//! SQLite authorizes intent and publication; neither is an engine lifetime.
use super::{
    owner::PersistedReceipt,
    repair,
    seal::{Generation, Intent, Seal, digest},
};
use crate::{error::AppResult, intelligence::jobs::now_ms, storage::LocalStore};
use rusqlite::{OptionalExtension, Transaction, params};
use std::sync::Arc;
use uuid::Uuid;

pub(crate) struct IndexRepository {
    store: Arc<LocalStore>,
}
#[derive(Clone, Debug)]
pub(crate) struct DurableIntent {
    body: Intent,
    encoded: String,
    prior: Option<String>,
}
impl DurableIntent {
    pub(super) fn body(&self) -> &Intent {
        &self.body
    }
    pub(super) fn prior(&self) -> AppResult<Option<Seal>> {
        self.prior.as_deref().map(Seal::decode).transpose()
    }
}
#[derive(Clone, Debug)]
pub(crate) struct PublicationAck {
    intent: DurableIntent,
    seal_json: String,
}
impl PublicationAck {
    pub(super) fn intent(&self) -> &DurableIntent {
        &self.intent
    }
    pub(super) fn seal_json(&self) -> &str {
        &self.seal_json
    }
}
impl IndexRepository {
    pub(crate) fn new(store: Arc<LocalStore>) -> Self {
        Self { store }
    }
    pub(super) fn can_retire(&self, target: &Generation, source: &Generation) -> AppResult<()> {
        self.store.transaction(|tx| {
            let valid: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM \
                    knowledge_index_generations WHERE \
                    identity=?1 AND active_generation=?2 AND \
                    active_generation!=?3)",
                params![
                    target.key()?,
                    target.generation.to_string(),
                    source.generation.to_string()
                ],
                |r| r.get(0),
            )?;
            if target.key()? != source.key()? || !valid {
                return Err(repair());
            }
            Ok(())
        })
    }
    pub(super) fn recovery_evidence(
        &self,
        identity: &str,
        generation: Uuid,
    ) -> AppResult<super::recovery::RecoveryEvidence> {
        self.store
            .transaction(|tx| super::recovery::evidence(tx, identity, generation))
    }
    pub(super) fn validate_recovery(
        &self,
        fence: &super::recovery::RecoveryFence,
        active: bool,
    ) -> AppResult<()> {
        self.store
            .transaction(|tx| super::recovery::validate(tx, fence, active))
    }
    pub(super) fn create(&self, generation: Generation, epoch: Uuid) -> AppResult<DurableIntent> {
        let body = Intent::new(generation, epoch, None)?;
        let intent = DurableIntent {
            encoded: body.encode()?,
            body,
            prior: None,
        };
        self.store.transaction(|tx| {
            tx.execute(
                "INSERT INTO \
                    knowledge_index_generations(identity,\
                    profile_id,dimension,state) VALUES (?1,?2,?3,\
                    'building') ON CONFLICT(identity) DO NOTHING",
                params![
                    intent.body.identity,
                    intent.body.descriptor.identity.profile_id.to_string(),
                    intent.body.descriptor.identity.dimensions
                ],
            )?;
            if read_row(tx, &intent.body)?.is_some() {
                return Err(repair());
            }
            tx.execute(
                "INSERT INTO \
                    knowledge_index_generation_seals(identity,\
                    generation,seal_json,dirty,intent_json,\
                    updated_at_ms) VALUES (?1,?2,NULL,1,?3,?4)",
                params![
                    intent.body.identity,
                    intent.body.descriptor.generation.to_string(),
                    intent.encoded,
                    now_ms()
                ],
            )?;
            Ok(())
        })?;
        Ok(intent)
    }
    pub(super) fn open(&self, generation: Generation, epoch: Uuid) -> AppResult<DurableIntent> {
        let identity = generation.key()?;
        self.store.transaction(|tx| {
            let row: Option<Row> = tx
                .query_row(
                    "SELECT seal_json,dirty,intent_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2",
                    params![identity, generation.generation.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((Some(prior), 0, None)) = row else {
                return Err(repair());
            };
            let seal = Seal::decode(&prior)?;
            if seal.descriptor != generation {
                return Err(repair());
            }
            let body = Intent::new(generation, epoch, Some(&prior))?;
            let intent = DurableIntent {
                encoded: body.encode()?,
                body,
                prior: Some(prior),
            };
            tx.execute(
                "UPDATE knowledge_index_generation_seals SET \
                    dirty=1,intent_json=?3,updated_at_ms=?4 \
                    WHERE identity=?1 AND generation=?2",
                params![
                    intent.body.identity,
                    intent.body.descriptor.generation.to_string(),
                    intent.encoded,
                    now_ms()
                ],
            )?;
            Ok(intent)
        })
    }
    pub(super) fn close(
        &self,
        receipt: &PersistedReceipt,
        epoch: Uuid,
    ) -> AppResult<DurableIntent> {
        self.store.transaction(|tx| {
            let Some((Some(prior), 0, None)) = read_row(tx, receipt.intent().body())? else {
                return Err(repair());
            };
            if prior != receipt.seal_json() {
                return Err(repair());
            }
            let body = Intent::close(receipt.intent().body().descriptor.clone(), epoch, &prior)?;
            let intent = DurableIntent {
                encoded: body.encode()?,
                body,
                prior: Some(prior),
            };
            tx.execute(
                "UPDATE knowledge_index_generation_seals SET \
                    dirty=1,intent_json=?3,updated_at_ms=?4 \
                    WHERE identity=?1 AND generation=?2",
                params![
                    intent.body.identity,
                    intent.body.descriptor.generation.to_string(),
                    intent.encoded,
                    now_ms()
                ],
            )?;
            Ok(intent)
        })
    }
    pub(super) fn validate(&self, intent: &DurableIntent) -> AppResult<()> {
        self.store.transaction(|tx| validate_intent(tx, intent))
    }
    /// Future mapping/pointer/Ready publication belongs in this same transaction
    /// through a typed publication plan. This layer does not alter document state.
    pub(crate) fn publish(&self, receipt: &PersistedReceipt) -> AppResult<PublicationAck> {
        self.store.transaction(|tx| {
            validate_intent(tx, receipt.intent())?;
            publish_seal(tx, receipt)
        })?;
        Ok(PublicationAck {
            intent: receipt.intent().clone(),
            seal_json: receipt.seal_json().to_owned(),
        })
    }
    pub(super) fn publish_plan(
        &self,
        receipt: &PersistedReceipt,
        plan: &super::publication::PublicationPlan,
    ) -> AppResult<PublicationAck> {
        self.store.transaction(|tx| {
            validate_intent(tx, receipt.intent())?;
            plan.apply(tx, receipt.intent().body())?;
            publish_seal(tx, receipt)
        })?;
        Ok(PublicationAck {
            intent: receipt.intent().clone(),
            seal_json: receipt.seal_json().to_owned(),
        })
    }
    pub(super) fn recover_ack(&self, receipt: &PersistedReceipt) -> AppResult<PublicationAck> {
        let ack = PublicationAck {
            intent: receipt.intent().clone(),
            seal_json: receipt.seal_json().to_owned(),
        };
        self.validate_ack(&ack)?;
        Ok(ack)
    }
    pub(super) fn validate_ack(&self, ack: &PublicationAck) -> AppResult<()> {
        self.store.transaction(|tx| {
            let Some((Some(seal), 0, None)) = read_row(tx, &ack.intent.body)? else {
                return Err(repair());
            };
            if seal != ack.seal_json {
                return Err(repair());
            }
            let decoded = Seal::decode(&seal)?;
            if decoded.operation_id != ack.intent.body.operation_id
                || decoded.descriptor != ack.intent.body.descriptor
            {
                return Err(repair());
            }
            Ok(())
        })
    }
}
type Row = (Option<String>, i64, Option<String>);
fn read_row(tx: &Transaction<'_>, intent: &Intent) -> AppResult<Option<Row>> {
    Ok(tx
        .query_row(
            "SELECT seal_json,dirty,intent_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2",
            params![intent.identity, intent.descriptor.generation.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}
fn validate_intent(tx: &Transaction<'_>, intent: &DurableIntent) -> AppResult<()> {
    let Some((prior, 1, Some(encoded))) = read_row(tx, &intent.body)? else {
        return Err(repair());
    };
    if encoded != intent.encoded
        || Intent::decode(&encoded)? != intent.body
        || prior != intent.prior
        || prior.as_deref().map(digest) != intent.body.prior_seal_sha256
    {
        return Err(repair());
    }
    Ok(())
}
fn publish_seal(tx: &Transaction<'_>, receipt: &PersistedReceipt) -> AppResult<()> {
    let intent = receipt.intent().body();
    let seal = Seal::decode(receipt.seal_json())?;
    if seal.operation_id != intent.operation_id || seal.descriptor != intent.descriptor {
        return Err(repair());
    }
    tx.execute(
        "UPDATE knowledge_index_generation_seals SET \
                    seal_json=?3,dirty=0,intent_json=NULL,\
                    updated_at_ms=?4 WHERE identity=?1 AND \
                    generation=?2",
        params![
            intent.identity,
            intent.descriptor.generation.to_string(),
            receipt.seal_json(),
            now_ms()
        ],
    )?;
    Ok(())
}

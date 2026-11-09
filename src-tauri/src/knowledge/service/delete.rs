use super::*;
use crate::knowledge::index::{Seal, delete_completion as completed, delete_work};
#[derive(Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteInput {
    #[serde(deserialize_with = "super::job_proof::canonical_uuid")]
    pub(crate) document_id: Uuid,
    pub(crate) document_version: u32,
    pub(crate) document: crate::knowledge::index::DocumentFence,
    #[serde(deserialize_with = "super::job_proof::canonical_uuid")]
    pub(crate) operation_id: Uuid,
    pub(crate) identity: Option<String>,
    #[serde(
        default,
        deserialize_with = "super::job_proof::optional_canonical_uuid"
    )]
    pub(crate) generation: Option<Uuid>,
}
impl DeleteInput {
    pub(crate) fn validate(&self) -> AppResult<()> {
        self.document.validate()?;
        if self.document.id != self.document_id
            || self.document.version != self.document_version
            || self.identity.is_some() != self.generation.is_some()
            || self.identity.as_ref().is_some_and(|i| {
                i.len() != 64
                    || !i
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err(error(Code::Configuration));
        }
        Ok(())
    }
}
pub(super) fn admit(service: &KnowledgeService, id: Uuid) -> AppResult<JobView> {
    let admission = service.jobs.admission_fence(JobKind::DeleteDocument)?;
    let job = service.core.store.transaction(|tx| {
        admission.run(|| {
            let f = state::fence(tx, id)?;
            let identity: Option<String> = tx.query_row(
                "SELECT index_identity FROM documents WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )?;
            let generation = identity
                .as_ref()
                .map(|key| {
                    tx.query_row(
                        "SELECT active_generation FROM \
                    knowledge_index_generations WHERE identity=?1",
                        [key],
                        |r| r.get::<_, Option<String>>(0),
                    )
                })
                .transpose()?
                .flatten()
                .map(|v| Uuid::parse_str(&v).map_err(|_| error(Code::Storage)))
                .transpose()?;
            let operation_id = Uuid::new_v4();
            let input = DeleteInput {
                document_id: id,
                document_version: f.version,
                document: f.clone(),
                operation_id,
                identity: identity.clone(),
                generation,
            };
            let job = admit_job(
                tx,
                JobDraft {
                    id: operation_id,
                    kind: JobKind::DeleteDocument,
                    session_id: None,
                    object_id: Some(id),
                    admission: JobAdmission::Manual {
                        exclusion_key: format!("document:{id}"),
                    },
                    payload: serde_json::json!({
                    "version":1,
                    "profiles":[],
                    "input":input}
                    ),
                    initial_failure: None,
                },
            )?
            .view()
            .clone();
            if let (Some(identity), Some(generation)) = (identity, generation) {
                let mut after = String::new();
                let mut ordinal = 0u32;
                loop {
                    let mut q = tx.prepare(
                        "SELECT vector_id,document_version,chunk_id,\
                    content_hash FROM \
                    knowledge_generation_mappings WHERE \
                    document_id=?1 AND identity=?2 AND \
                    generation=?3 AND published=1 AND \
                    chunk_id>?4 ORDER BY chunk_id LIMIT 200",
                    )?;
                    let mut rows = q.query(params![
                        id.to_string(),
                        identity,
                        generation.to_string(),
                        after
                    ])?;
                    let mut page = 0;
                    while let Some(r) = rows.next()? {
                        if ordinal >= 200000 {
                            return Err(error(Code::ResourceLimit));
                        }
                        tx.execute(
                            "INSERT INTO \
                    knowledge_index_work_items(operation_id,\
                    ordinal,identity,generation,vector_id,\
                    document_id,document_version,chunk_id,\
                    content_hash,document_fence_json) VALUES (?1,\
                    ?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                            params![
                                operation_id.to_string(),
                                ordinal,
                                identity,
                                generation.to_string(),
                                state::text(r, 0, 20)?,
                                id.to_string(),
                                r.get::<_, u32>(1)?,
                                state::text(r, 2, 36)?,
                                state::text(r, 3, 64)?,
                                serde_json::to_string(&f).map_err(|_| error(Code::Storage))?
                            ],
                        )?;
                        after = state::text(r, 2, 36)?.into();
                        ordinal += 1;
                        page += 1;
                    }
                    if page == 0 {
                        break;
                    }
                }
            }
            tx.execute(
                "UPDATE documents SET status='deleting' WHERE id=?1",
                [id.to_string()],
            )?;
            tx.execute(
                "UPDATE document_chunks SET index_state='deleting' WHERE document_id=?1",
                [id.to_string()],
            )?;
            Ok(job)
        })
    })?;
    service.jobs.event_port().publish(job.clone());
    service.jobs.wake();
    Ok(job)
}
pub(super) struct DeleteHandler(pub(super) Arc<Core>);
#[async_trait::async_trait]
impl JobHandler for DeleteHandler {
    async fn run(&self, context: JobContext, payload: serde_json::Value) -> Result<(), Code> {
        let e = decode_payload(&serde_json::to_string(&payload).map_err(|_| Code::Configuration)?)
            .map_err(code)?;
        if !e.profiles.is_empty() {
            return Err(Code::Configuration);
        }
        let input: DeleteInput =
            serde_json::from_value(e.input).map_err(|_| Code::Configuration)?;
        input.validate().map_err(code)?;
        if input.operation_id != context.id {
            return Err(Code::Configuration);
        }
        let owner = context.operation_owner().map_err(code)?;
        let _serial = self.0.serial.lock().await;
        self.0
            .delete(&input, owner, Some(&context))
            .await
            .map_err(code)
    }
    fn terminal_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        id: Uuid,
        status: JobStatus,
        _code: Option<Code>,
        awaiting_reap: bool,
    ) -> AppResult<()> {
        completed::consume(tx, id, status, awaiting_reap, &self.0.reap)
    }
    async fn cleanup(&self, reaped: Option<Uuid>) -> AppResult<()> {
        if let Some(id) = reaped {
            self.0.store.transaction(|tx| {
                if let Some(job) = job_proof::local_delete(tx, id)? {
                    completed::consume(tx, id, job.status, false, &self.0.reap)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
}
impl Core {
    pub(crate) async fn delete(
        &self,
        input: &DeleteInput,
        owner: OperationLease,
        context: Option<&JobContext>,
    ) -> AppResult<()> {
        input.validate()?;
        let fence = input.document.clone();
        let proof = self.store.transaction(|tx| {
            let validate = || {
                let job = job_proof::delete(tx, input.operation_id)?
                    .ok_or_else(|| error(Code::Configuration))?;
                if job.input != *input {
                    return Err(error(Code::Configuration));
                }
                Ok(job.proof)
            };
            match context {
                Some(context) => context.commit(tx, validate),
                None => validate(),
            }
        })?;
        if self
            .store
            .transaction(|tx| completed::load(tx, input.operation_id))?
            .is_some()
        {
            return self.finish_completion(input, owner).await;
        }
        if fence.version != input.document_version
            || !self.store.transaction(|tx| fence.matches(tx, "deleting"))?
        {
            return Err(error(Code::Cancelled));
        }
        let cleanup_directory =
            completed::resolve_directory(self.data_dir.clone(), owner.clone()).await?;
        let work_proof = self
            .store
            .transaction(|tx| completed::preflight(tx, input, &cleanup_directory))?;
        if let (Some(identity), Some(generation)) = (&input.identity, input.generation) {
            let descriptor = self.store.transaction(|tx| {
                let mut q = tx.prepare(
                    "SELECT seal_json FROM \
                    knowledge_index_generation_seals WHERE \
                    identity=?1 AND generation=?2 AND dirty=0",
                )?;
                let mut rows = q.query(params![identity, generation.to_string()])?;
                let r = rows
                    .next()?
                    .ok_or_else(|| error(Code::IndexRepairRequired))?;
                let seal = Seal::decode(state::text(r, 0, 65536)?)?;
                if seal.descriptor.key()? != *identity || seal.descriptor.generation != generation {
                    return Err(error(Code::IndexRepairRequired));
                }
                Ok(seal.descriptor)
            })?;
            let ticket = self.index.open_existing(descriptor, owner.clone())?;
            self.index.enqueue_open(&ticket)?.observe().await?;
            for page in 0..work_proof.count.div_ceil(delete_work::PAGE_SIZE) {
                let rows = self.store.transaction(|tx| {
                    proof.require(tx, &fence)?;
                    work_proof.page(tx, input, page, true)
                })?;
                let ids = rows.iter().map(|row| row.vector).collect();
                self.index.delete(&ticket, ids).await?;
                tokio::task::yield_now().await;
            }
            let receipt = self.index.persist(&ticket).await?;
            #[cfg(test)]
            self.checkpoint(CommitPoint::AfterVectorPersist)?;
            let plan = PublicationPlan::Delete {
                operation: ticket.operation_id(),
                work: input.operation_id,
                document: fence,
                job: context.map(JobContext::publication_fence),
                job_proof: proof.clone(),
                work_proof: work_proof.clone(),
                cleanup_directory,
            };
            let ack = self.index.publish_plan(&ticket, &receipt, plan).await?;
            self.index.acknowledge(&ticket, ack).await?;
        } else {
            self.store.transaction(|tx| {
                let remove = || {
                    proof.require(tx, &fence)?;
                    work_proof.require_live(tx, input)?;
                    if !fence.matches(tx, "deleting")? {
                        return Err(error(Code::Cancelled));
                    }
                    completed::publish_delete(tx, input, None, &cleanup_directory)
                };
                match context {
                    Some(c) => c.commit(tx, remove),
                    None => remove(),
                }
            })?;
        }
        #[cfg(test)]
        self.checkpoint(CommitPoint::AfterDeletePublication)?;
        self.finish_completion(input, owner).await
    }
}

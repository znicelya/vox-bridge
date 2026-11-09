//! Resume committed domain completion even after its document row is gone.
use super::*;
use crate::knowledge::index::{Generation, Seal, delete_completion as completed};

impl Core {
    pub(crate) async fn finish_completion(
        &self,
        input: &DeleteInput,
        owner: OperationLease,
    ) -> AppResult<()> {
        let (publication_id, source) = self.store.transaction(|tx| {
            let value = completed::load(tx, input.operation_id)?
                .ok_or_else(|| error(Code::IndexRepairRequired))?;
            if value.header.input != *input {
                return Err(error(Code::Configuration));
            }
            let publication = completed::read_publication(tx, value.header.publication)?;
            Ok((
                value.header.publication,
                publication.authority.source_descriptor()?,
            ))
        })?;
        match source {
            Some(source) => {
                self.finish_source_deletes(&source, owner).await?;
            }
            None => {
                self.prepare_completion_files(owner.clone()).await?;
                self.finish_publication_work(publication_id, owner.clone())
                    .await?;
                self.collect_publication(publication_id, owner).await?;
            }
        }
        Ok(())
    }
    async fn collect_publication(
        &self,
        publication_id: Uuid,
        owner: OperationLease,
    ) -> AppResult<()> {
        let ready = self.store.transaction(|tx| {
            completed::settle_local(tx, publication_id, &self.reap)?;
            completed::collection_ready(tx, publication_id)
        })?;
        if !ready {
            return Ok(());
        }
        let receipt = completed::check_collection(
            self.store.clone(),
            self.data_dir.clone(),
            publication_id,
            owner.clone(),
        )
        .await?;
        // No filesystem call occurs while SQLite is held. This single-use
        // current receipt remains owned through the atomic complete-cohort GC.
        self.store
            .transaction(|tx| completed::collect(tx, publication_id, receipt))
    }
    pub(super) async fn prepare_completion_files(&self, owner: OperationLease) -> AppResult<()> {
        let mut after = String::new();
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_operations(tx, &after))?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                after = id.to_string();
                let value = self
                    .store
                    .transaction(|tx| completed::load(tx, id))?
                    .ok_or_else(|| error(Code::IndexRepairRequired))?;
                completed::check_files(
                    self.store.clone(),
                    self.data_dir.clone(),
                    id,
                    owner.clone(),
                    value.files_cleaned,
                )
                .await?;
            }
        }
        let mut after = String::new();
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_publications(tx, &after))?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                after = id.to_string();
                if self
                    .store
                    .transaction(|tx| completed::optional_publication(tx, id))?
                    .is_some()
                {
                    self.store
                        .transaction(|tx| completed::validate_complete(tx, id))?;
                }
            }
        }
        crate::knowledge::documents::cleanup_managed(
            self.store.clone(),
            self.data_dir.clone(),
            owner,
        )
        .await
    }
    async fn finish_publication_work(
        &self,
        publication_id: Uuid,
        owner: OperationLease,
    ) -> AppResult<()> {
        let publication = self
            .store
            .transaction(|tx| completed::read_publication(tx, publication_id))?;
        for batch in 0..publication.batches {
            let ids = self
                .store
                .transaction(|tx| completed::operations(tx, publication_id, batch))?;
            for id in ids {
                let value = self
                    .store
                    .transaction(|tx| completed::load(tx, id))?
                    .ok_or_else(|| error(Code::IndexRepairRequired))?;
                let receipt = completed::check_files(
                    self.store.clone(),
                    self.data_dir.clone(),
                    id,
                    owner.clone(),
                    true,
                )
                .await?
                .ok_or_else(|| error(Code::IndexRepairRequired))?;
                if !value.files_cleaned {
                    self.store
                        .transaction(|tx| completed::mark_files_cleaned(tx, id, &receipt))?;
                }
            }
            self.store
                .transaction(|tx| completed::acknowledge_batch(tx, publication_id, batch))?;
        }
        Ok(())
    }
    pub(crate) async fn finish_source_deletes(
        &self,
        source: &Generation,
        owner: OperationLease,
    ) -> AppResult<bool> {
        let mut after = String::new();
        let mut found = false;
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_publications(tx, &after))?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                after = id.to_string();
                let Some(publication) = self
                    .store
                    .transaction(|tx| completed::optional_publication(tx, id))?
                else {
                    continue;
                };
                if publication.authority.identity.as_deref() == Some(source.key()?.as_str())
                    && publication.authority.source == Some(source.generation)
                {
                    found = true;
                    self.store
                        .transaction(|tx| completed::validate_complete(tx, id))?;
                }
            }
        }
        if !found {
            return Ok(false);
        }
        self.prepare_completion_files(owner.clone()).await?;
        let active = self.store.transaction(|tx| {
            let mut q = tx.prepare(
                "SELECT active_generation FROM knowledge_index_generations WHERE identity=?1",
            )?;
            let mut rows = q.query([source.key()?])?;
            let raw = state::text(
                rows.next()?
                    .ok_or_else(|| error(Code::IndexRepairRequired))?,
                0,
                36,
            )?;
            let id = Uuid::parse_str(raw).map_err(|_| error(Code::IndexRepairRequired))?;
            if id.to_string() != raw {
                return Err(error(Code::IndexRepairRequired));
            }
            Ok(id)
        })?;
        if active != source.generation {
            let descriptor = self.store.transaction(|tx| {
                let mut q = tx.prepare("SELECT seal_json FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2 AND dirty=0")?;
                let mut rows = q.query(params![source.key()?, active.to_string()])?;
                let seal = Seal::decode(state::text(rows.next()?.ok_or_else(|| error(Code::IndexRepairRequired))?, 0, 65536)?)?;
                if seal.descriptor.key()? != source.key()? || seal.descriptor.generation != active { return Err(error(Code::IndexRepairRequired)) }
                Ok(seal.descriptor)
            })?;
            let ticket = self.index.open_existing(descriptor, owner.clone())?;
            self.index.enqueue_open(&ticket)?.observe().await?;
            let receipt = self.index.persist(&ticket).await?;
            let ack = self.index.publish(&ticket, &receipt).await?;
            self.index.acknowledge(&ticket, ack).await?;
            self.index
                .remove_generation(&ticket, source.clone())
                .await?;
        }
        let mut after = String::new();
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_publications(tx, &after))?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                after = id.to_string();
                let Some(publication) = self
                    .store
                    .transaction(|tx| completed::optional_publication(tx, id))?
                else {
                    continue;
                };
                if publication.authority.identity.as_deref() == Some(source.key()?.as_str())
                    && publication.authority.source == Some(source.generation)
                {
                    self.finish_publication_work(id, owner.clone()).await?;
                }
            }
        }
        if active != source.generation {
            self.remove_generation_rows(source)?
        }
        let mut after = String::new();
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_publications(tx, &after))?;
            if ids.is_empty() {
                break;
            }
            for id in ids {
                after = id.to_string();
                let Some(publication) = self
                    .store
                    .transaction(|tx| completed::optional_publication(tx, id))?
                else {
                    continue;
                };
                if publication.authority.identity.as_deref() == Some(source.key()?.as_str())
                    && publication.authority.source == Some(source.generation)
                {
                    self.collect_publication(id, owner.clone()).await?;
                }
            }
        }
        Ok(true)
    }
    pub(crate) async fn resume_completions(&self, owner: OperationLease) -> AppResult<()> {
        let mut after = String::new();
        loop {
            let inputs = self.store.transaction(|tx| {
                completed::scan_operations(tx, &after)?
                    .into_iter()
                    .map(|id| Ok((id, completed::resume_input(tx, id)?)))
                    .collect::<AppResult<Vec<_>>>()
            })?;
            if inputs.is_empty() {
                break;
            }
            for (id, input) in inputs {
                after = id.to_string();
                let Some(input) = input else { continue };
                if self
                    .store
                    .transaction(|tx| completed::resume_input(tx, id))?
                    .is_some()
                {
                    self.finish_completion(&input, owner.clone()).await?;
                }
            }
        }
        // Consumed successful Direct Delete may be collectible on an active
        // source. Scan publications once instead of replaying settled peers.
        let mut after = String::new();
        loop {
            let ids = self
                .store
                .transaction(|tx| completed::scan_publications(tx, &after))?;
            if ids.is_empty() {
                return Ok(());
            }
            for id in ids {
                after = id.to_string();
                self.collect_publication(id, owner.clone()).await?;
            }
        }
    }
}

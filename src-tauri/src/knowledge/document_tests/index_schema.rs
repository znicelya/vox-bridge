//! Storage-only prerequisites for Task5. No engine, embeddings or application
//! mapping-publication behavior is implemented or certified by these tests.
use super::*;
const IDENTITY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const GENERATION: &str = "11111111-1111-4111-8111-111111111111";
const WORK: &str = "INSERT INTO knowledge_index_work_items (operation_id,ordinal,identity,generation,vector_id,document_id,document_version,chunk_id,content_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)";
const MAP: &str = "INSERT INTO knowledge_generation_mappings (identity,generation,chunk_id,document_id,document_version,vector_id,content_hash,published) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)";
fn generation(tx: &rusqlite::Transaction<'_>, value: &str) -> AppResult<()> {
    tx.execute("INSERT INTO knowledge_index_generations (identity,profile_id,dimension,active_generation,state) VALUES (?1,NULL,128,NULL,'repair_required') ON CONFLICT(identity) DO NOTHING",[IDENTITY])?;
    tx.execute("INSERT INTO knowledge_index_generation_seals (identity,generation,seal_json,dirty,intent_json,updated_at_ms) VALUES (?1,?2,'{\"version\":1}',0,NULL,1)",params![IDENTITY,value])?;
    Ok(())
}
#[test]
fn documents_index_work_schema_bounds_rows_keys_ids_and_snapshot_values() {
    let store = LocalStore::in_memory().unwrap();
    store
        .transaction(|tx| {
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM knowledge_index_work_items",
                    [],
                    |row| row.get::<_, u32>(0)
                )?,
                0
            );
            assert_eq!(
                tx.query_row(
                    "SELECT count(*) FROM knowledge_generation_mappings",
                    [],
                    |row| row.get::<_, u32>(0)
                )?,
                0
            );
            generation(tx, GENERATION)?;
            let operation = Uuid::new_v4().to_string();
            let document = Uuid::new_v4().to_string();
            let chunk = Uuid::new_v4().to_string();
            for id in ["", "-1", "01", "18446744073709551616", "1.0", "１"] {
                assert!(
                    tx.execute(
                        WORK,
                        params![
                            operation,
                            0,
                            IDENTITY,
                            GENERATION,
                            id,
                            None::<String>,
                            None::<u32>,
                            None::<String>,
                            None::<String>
                        ]
                    )
                    .is_err()
                );
                assert!(
                    tx.execute(
                        MAP,
                        params![IDENTITY, GENERATION, chunk, document, 1, id, "hash", 0]
                    )
                    .is_err()
                );
            }
            for ordinal in [-1_i64, 200000] {
                assert!(
                    tx.execute(
                        WORK,
                        params![
                            operation,
                            ordinal,
                            IDENTITY,
                            GENERATION,
                            "0",
                            None::<String>,
                            None::<u32>,
                            None::<String>,
                            None::<String>
                        ]
                    )
                    .is_err()
                );
            }
            assert!(
                tx.execute(
                    WORK,
                    params![
                        operation,
                        0.5,
                        IDENTITY,
                        GENERATION,
                        "0",
                        None::<String>,
                        None::<u32>,
                        None::<String>,
                        None::<String>
                    ]
                )
                .is_err()
            );
            for version in [0_i64, -1, 4294967296] {
                assert!(
                    tx.execute(
                        MAP,
                        params![
                            IDENTITY, GENERATION, chunk, document, version, "0", "hash", 0
                        ]
                    )
                    .is_err()
                );
                assert!(
                    tx.execute(
                        WORK,
                        params![
                            operation, 0, IDENTITY, GENERATION, "0", document, version, chunk,
                            "hash"
                        ]
                    )
                    .is_err()
                );
            }
            assert!(
                tx.execute(
                    MAP,
                    params![IDENTITY, GENERATION, chunk, document, 1, "0", "hash", 2]
                )
                .is_err()
            );
            assert!(
                tx.execute(
                    WORK,
                    params![
                        "x".repeat(37),
                        0,
                        IDENTITY,
                        GENERATION,
                        "0",
                        None::<String>,
                        None::<u32>,
                        None::<String>,
                        None::<String>
                    ]
                )
                .is_err()
            );
            tx.execute(
                WORK,
                params![
                    operation,
                    0,
                    IDENTITY,
                    GENERATION,
                    "18446744073709551615",
                    document,
                    1,
                    chunk,
                    "hash"
                ],
            )?;
            tx.execute(
                MAP,
                params![
                    IDENTITY,
                    GENERATION,
                    chunk,
                    document,
                    1,
                    "18446744073709551615",
                    "hash",
                    0
                ],
            )?;
            assert_eq!(
                tx.query_row(
                    "SELECT vector_id,typeof(vector_id) FROM knowledge_index_work_items",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                )?,
                ("18446744073709551615".into(), "text".into())
            );
            assert!(
                tx.execute(
                    WORK,
                    params![
                        operation,
                        0,
                        IDENTITY,
                        GENERATION,
                        "0",
                        None::<String>,
                        None::<u32>,
                        None::<String>,
                        None::<String>
                    ]
                )
                .is_err()
            );
            assert!(
                tx.execute(
                    MAP,
                    params![
                        IDENTITY,
                        GENERATION,
                        Uuid::new_v4().to_string(),
                        document,
                        1,
                        "18446744073709551615",
                        "hash",
                        0
                    ]
                )
                .is_err()
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn documents_index_work_survives_canonical_deletion_and_pages_high_ids() {
    let store = LocalStore::in_memory().unwrap();
    store.transaction(|tx| {
        generation(tx,GENERATION)?;
        let operation=Uuid::new_v4().to_string();let document=Uuid::new_v4().to_string();let chunk=Uuid::new_v4().to_string();let job=Uuid::new_v4().to_string();
        tx.execute("INSERT INTO documents (id,sha256,uri,title,created_at) VALUES (?1,'hash','legacy-reference',NULL,37)",[&document])?;
        tx.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text) VALUES (?1,?2,0,'old text')",params![chunk,document])?;
        tx.execute("INSERT INTO intelligence_jobs (id,kind,status,object_id,exclusion_key,payload_json,created_at_ms,updated_at_ms) VALUES (?1,'import','succeeded',?2,'document:fixture','{\"version\":1,\"profiles\":[],\"input\":{}}',1,1)",params![job,document])?;
        let ids:Vec<_>=(190000_u64..200000).collect();
        assert_eq!(serde_json::to_vec(&ids).unwrap().len(),70001,"bulk IDs do not fit the job or intent envelope");
        let mut insert=tx.prepare(WORK)?;
        for (ordinal,id) in ids.iter().enumerate() {
            insert.execute(params![operation,ordinal as i64,IDENTITY,GENERATION,id.to_string(),document,1,chunk,"hash"])?;
        }
        drop(insert);
        tx.execute(MAP,params![IDENTITY,GENERATION,chunk,document,1,"190000","hash",1])?;
        tx.execute("DELETE FROM document_chunks WHERE document_id=?1",[&document])?;
        tx.execute("DELETE FROM documents WHERE id=?1",[&document])?;
        tx.execute("DELETE FROM intelligence_jobs WHERE id=?1",[&job])?;
        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_generation_mappings",[],|row|row.get::<_,u32>(0))?,1);
        let mut next=0_i64;
        loop {
            let rows=tx.prepare("SELECT ordinal,vector_id FROM knowledge_index_work_items WHERE operation_id=?1 AND ordinal>=?2 ORDER BY ordinal LIMIT 200")?
                .query_map(params![operation,next],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
            assert!(rows.len()<=200);
            if rows.is_empty(){break;}
            for (ordinal,id) in rows {assert_eq!(ordinal,next);assert_eq!(id,(190000+next).to_string());next+=1;}
        }
        assert_eq!(next,10000);
        assert!(tx.execute("DELETE FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",params![IDENTITY,GENERATION]).is_err());
        assert!(tx.execute("DELETE FROM knowledge_index_generations WHERE identity=?1",[IDENTITY]).is_err());
        tx.execute("DELETE FROM knowledge_index_work_items WHERE operation_id=?1",[operation])?;
        assert!(tx.execute("DELETE FROM knowledge_index_generation_seals WHERE identity=?1 AND generation=?2",params![IDENTITY,GENERATION]).is_err());
        tx.execute("DELETE FROM knowledge_generation_mappings WHERE identity=?1 AND generation=?2",params![IDENTITY,GENERATION])?;
        tx.execute("DELETE FROM knowledge_index_generations WHERE identity=?1",[IDENTITY])?;
        Ok(())
    }).unwrap();
}
#[test]
fn documents_index_mapping_schema_supports_current_generation_and_version_join() {
    let store = LocalStore::in_memory().unwrap();
    store.transaction(|tx| {
        generation(tx,GENERATION)?;
        let current=Uuid::new_v4().to_string();generation(tx,&current)?;
        let document=Uuid::new_v4().to_string();let old_chunk=Uuid::new_v4().to_string();let chunk=Uuid::new_v4().to_string();
        tx.execute("INSERT INTO documents (id,sha256,uri,created_at,kind,origin,version,status,index_identity) VALUES (?1,'hash','managed',1,'text','native',2,'ready',?2)",params![document,IDENTITY])?;
        tx.execute("INSERT INTO document_chunks (id,document_id,chunk_index,text,document_version,content_hash,index_state) VALUES (?1,?2,0,'new text',2,'newhash','ready')",params![chunk,document])?;
        tx.execute(MAP,params![IDENTITY,GENERATION,old_chunk,document,1,"3","oldhash",1])?;
        tx.execute(MAP,params![IDENTITY,current,chunk,document,2,"4","newhash",0])?;
        tx.execute("UPDATE knowledge_index_generations SET active_generation=?1,state='ready' WHERE identity=?2",params![current,IDENTITY])?;
        let query="SELECT count(*) FROM knowledge_generation_mappings m JOIN knowledge_index_generations g ON g.identity=m.identity AND g.active_generation=m.generation AND g.state='ready' JOIN documents d ON d.id=m.document_id AND d.version=m.document_version AND d.index_identity=m.identity AND d.status='ready' JOIN document_chunks c ON c.id=m.chunk_id AND c.document_id=m.document_id AND c.document_version=m.document_version AND c.content_hash=m.content_hash WHERE m.published=1";
        assert_eq!(tx.query_row(query,[],|row|row.get::<_,u32>(0))?,0);
        tx.execute("UPDATE knowledge_generation_mappings SET published=1 WHERE identity=?1 AND generation=?2",params![IDENTITY,current])?;
        assert_eq!(tx.query_row(query,[],|row|row.get::<_,u32>(0))?,1);
        tx.execute("UPDATE documents SET version=3,status='parsing' WHERE id=?1",[document])?;
        assert_eq!(tx.query_row(query,[],|row|row.get::<_,u32>(0))?,0);
        assert_eq!(tx.query_row("SELECT count(*) FROM knowledge_generation_mappings",[],|row|row.get::<_,u32>(0))?,2);
        Ok(())
    }).unwrap();
}

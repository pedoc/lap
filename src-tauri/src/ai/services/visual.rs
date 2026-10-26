//! Original-image and sampled-video vectors have no dependency on generated analyses.
use super::{
    client,
    config::Resolved,
    store::{self, Hit, MediaSource, sql_error},
};
use rusqlite::{Connection, params};
use serde::Serialize;

pub fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS media_ai_visual_vectors(
      id TEXT PRIMARY KEY,file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
      stamp TEXT NOT NULL,source TEXT NOT NULL,segment INTEGER NOT NULL,start REAL NOT NULL,end REAL NOT NULL,duration REAL NOT NULL,
      profile_id TEXT NOT NULL REFERENCES media_ai_profiles(id) ON DELETE CASCADE,
      vector BLOB NOT NULL,created_at INTEGER NOT NULL,UNIQUE(file_id,stamp,segment,profile_id));
      CREATE INDEX IF NOT EXISTS media_ai_visual_profile ON media_ai_visual_vectors(profile_id);
      CREATE INDEX IF NOT EXISTS media_ai_visual_file ON media_ai_visual_vectors(file_id,stamp);")
      .map_err(sql_error)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualVector {
    pub id: String,
    pub segment: i64,
    pub start: f64,
    pub end: f64,
    pub duration: f64,
    pub created_at: i64,
}

pub fn vectors(
    c: &Connection,
    src: &MediaSource,
    model: &Resolved,
) -> Result<Vec<VisualVector>, String> {
    c.prepare("SELECT id,segment,start,end,duration,created_at FROM media_ai_visual_vectors WHERE file_id=?1 AND stamp=?2 AND profile_id=?3 ORDER BY segment")
      .map_err(sql_error)?.query_map(params![src.file_id,src.stamp(),model.multimodal_profile()?], |r| Ok(VisualVector {
          id:r.get(0)?,segment:r.get(1)?,start:r.get(2)?,end:r.get(3)?,duration:r.get(4)?,created_at:r.get(5)?,
      })).map_err(sql_error)?.collect::<Result<_,_>>().map_err(sql_error)
}

pub fn complete(src: &MediaSource, vectors: &[VisualVector]) -> bool {
    let duration = if src.kind == 2 {
        if src.duration > 0 {
            src.duration as f64
        } else {
            vectors.first().map_or(0.0, |v| v.duration)
        }
    } else {
        0.0
    };
    let count = if src.kind == 2 {
        if !duration.is_finite() || duration <= 0.0 || duration > 86400.0 {
            return false;
        }
        (duration / 60.0).ceil() as usize
    } else {
        1
    };
    vectors.len() == count
        && vectors.iter().enumerate().all(|(i, v)| {
            v.segment == i as i64
                && v.duration == duration
                && if src.kind == 2 {
                    v.start == i as f64 * 60.0 && v.end == ((i + 1) as f64 * 60.0).min(duration)
                } else {
                    v.start == 0.0 && v.end == 0.0
                }
        })
}

pub fn put_vector(
    c: &mut Connection,
    src: &MediaSource,
    model: &Resolved,
    segment: i64,
    start: f64,
    end: f64,
    duration: f64,
    vector: Vec<f32>,
) -> Result<(), String> {
    if segment < 0
        || !start.is_finite()
        || !end.is_finite()
        || !duration.is_finite()
        || start < 0.0
        || end < start
        || (src.kind == 2
            && (duration <= 0.0
                || duration > 86400.0
                || start != segment as f64 * 60.0
                || end <= start
                || end != (start + 60.0).min(duration)))
        || (src.kind != 2 && (segment != 0 || start != 0.0 || end != 0.0 || duration != 0.0))
    {
        return Err("原画面向量片段范围无效".into());
    }
    let vector = client::normalize(vector, model.model.embedding.dimensions)?;
    let profile = model.multimodal_profile()?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    if store::source(&tx, src.file_id)?.stamp() != src.stamp() {
        return Err("媒体或可见性已变化，原画面向量未写入".into());
    }
    src.verify()?;
    let snapshot = serde_json::to_string(src).map_err(|_| "来源快照无效")?;
    tx.execute(
        "INSERT OR IGNORE INTO media_ai_profiles VALUES(?1,?2,?3)",
        params![
            profile,
            serde_json::to_string(model).map_err(|_| "索引快照无效")?,
            chrono::Utc::now().timestamp()
        ],
    )
    .map_err(sql_error)?;
    let bytes: Vec<u8> = vector.iter().flat_map(|v| v.to_le_bytes()).collect();
    tx.execute("INSERT INTO media_ai_visual_vectors(id,file_id,stamp,source,segment,start,end,duration,profile_id,vector,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(file_id,stamp,segment,profile_id) DO UPDATE SET source=excluded.source,start=excluded.start,end=excluded.end,duration=excluded.duration,vector=excluded.vector,created_at=excluded.created_at",
      params![uuid::Uuid::new_v4().to_string(),src.file_id,src.stamp(),snapshot,segment,start,end,duration,profile,bytes,chrono::Utc::now().timestamp_millis()]).map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}

pub fn search(
    c: &Connection,
    model: &Resolved,
    query: &[f32],
    limit: usize,
    album_id: Option<i64>,
    file_type: i64,
) -> Result<Vec<Hit>, String> {
    let sql=format!("SELECT a.id,v.id,COALESCE(a.title,a.name),COALESCE(a.comments,''),v.start,v.end,v.vector
      FROM media_ai_visual_vectors v JOIN afiles a ON a.id=v.file_id JOIN afolders b ON b.id=a.folder_id
      WHERE v.profile_id=?1 AND (?2 IS NULL OR b.album_id=?2) AND (?3=0 OR (?3 & CASE a.file_type WHEN 1 THEN 1 WHEN 2 THEN 2 WHEN 3 THEN 4 ELSE 0 END)!=0)
      AND {} AND a.file_type IN(1,2,3)
      AND json_extract(v.source,'$.kind')=a.file_type AND json_extract(v.source,'$.path')=b.path||'/'||a.name
      AND json_extract(v.source,'$.size')=a.size AND json_extract(v.source,'$.modified') IS a.modified_at
      AND json_extract(v.source,'$.orientation')=COALESCE(a.e_orientation,1)",crate::ai::media::visible());
    let mut statement = c.prepare(&sql).map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![model.multimodal_profile()?, album_id, file_type],
            |r| {
                Ok((
                    Hit {
                        file_id: r.get(0)?,
                        analysis_id: r.get(1)?,
                        title: r.get(2)?,
                        description: r.get(3)?,
                        start: r.get(4)?,
                        end: r.get(5)?,
                        score: 0.0,
                    },
                    r.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .map_err(sql_error)?;
    store::rank_vectors(rows, query, limit, store::SearchUnit::Media)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ai::services::config::{EmbeddingProtocol, ProviderKind};

    fn model() -> Resolved {
        let mut model = store::tests::model("qwen-vl-embedding");
        model.provider.kind = ProviderKind::Embedding;
        model.provider.embedding_protocol = EmbeddingProtocol::VllmMultimodal;
        model.model.capabilities = model.provider.embedding_capabilities();
        model
    }

    pub(crate) fn embedding_reply(
        protocol: EmbeddingProtocol,
        vector: Vec<f32>,
        frames: usize,
    ) -> serde_json::Value {
        use serde_json::json;
        match protocol {
            EmbeddingProtocol::DashscopeMultimodal => {
                json!({"output":{"embeddings":[{"index":0,"embedding":vector,"type":"fusion"}]}})
            }
            EmbeddingProtocol::ArkMultimodal => json!({"data":{"embedding":vector}}),
            EmbeddingProtocol::SiliconflowMultimodal if frames > 1 => {
                json!({"data":(0..frames).map(|index|json!({"index":index,"embedding":vector})).collect::<Vec<_>>()})
            }
            _ => json!({"data":[{"index":0,"embedding":vector}]}),
        }
    }

    #[tokio::test]
    async fn domestic_providers_connect_llm_analysis_text_and_original_image_search() {
        use super::super::workflow;
        use genai::chat::ContentPart;
        use serde_json::json;
        for protocol in [
            EmbeddingProtocol::DashscopeMultimodal,
            EmbeddingProtocol::ArkMultimodal,
            EmbeddingProtocol::SiliconflowMultimodal,
        ] {
            let (mut c, dir) = store::tests::fixture();
            let path = dir.path().join("photo-1.jpg");
            image::RgbImage::from_pixel(48, 32, image::Rgb([40, 180, 80]))
                .save(&path)
                .unwrap();
            let original = blake3::hash(&std::fs::read(&path).unwrap());
            let info = crate::t_utils::FileInfo::new(path.to_str().unwrap()).unwrap();
            c.execute(
                "UPDATE afiles SET size=?1,modified_at=?2 WHERE id=1",
                params![info.file_size, info.modified],
            )
            .unwrap();
            let source = store::source(&c, 1).unwrap();
            let description =
                r#"{"title":"绿色照片","description":"一张绿色的图片","tags":["绿色"]}"#;
            let completion = |text: &str| json!({"id":"fixture","object":"chat.completion","model":"fixture-llm","choices":[{"index":0,"message":{"role":"assistant","content":text},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}});
            let (base, worker) = super::super::tests::fixture(vec![
                (200, completion(description)),
                (200, embedding_reply(protocol, vec![3.0, 4.0], 1)),
                (200, embedding_reply(protocol, vec![3.0, 4.0], 1)),
                (200, completion("这是一张绿色图片。[E1]")),
                (200, embedding_reply(protocol, vec![3.0, 4.0], 1)),
                (200, embedding_reply(protocol, vec![3.0, 4.0], 1)),
            ]);
            let mut llm = store::tests::model("fixture-llm");
            llm.provider.base_url = base.clone();
            let mut embedding = model();
            embedding.provider.base_url = base;
            embedding.provider.embedding_protocol = protocol;
            embedding.model.remote_id = "custom-native-model-alias".into();
            let input = workflow::parts(&source, 0.0, 0.0, "Describe this picture as JSON".into())
                .await
                .unwrap();
            let text = client::chat_with_key(&llm, input, None).await.unwrap();
            store::put_analysis(
                &mut c,
                &source,
                0,
                0.0,
                0.0,
                store::AnalysisText::parse(&text).unwrap(),
                &llm,
            )
            .unwrap();
            let analysis = store::analyses(&c, &source).unwrap().remove(0);
            let vector = client::embed_with_key(&embedding, vec![analysis.text.document()], None)
                .await
                .unwrap()
                .remove(0);
            store::put_vector(&c, &analysis, &embedding, vector).unwrap();
            let query = client::embed_text_query_with_key(&embedding, "绿色图片".into(), None)
                .await
                .unwrap();
            let db = dir.path().join("native-pipeline.db");
            c.execute("VACUUM INTO ?1", [db.to_str().unwrap()]).unwrap();
            drop(c);
            let mut c = Connection::open(db).unwrap();
            c.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            let hit = store::search(&c, &embedding, &query, 10, None, 0)
                .unwrap()
                .remove(0);
            assert_eq!(hit.file_id, 1);
            assert!((hit.score - 1.0).abs() < 1e-6);
            let answer = client::chat_with_key(
                &llm,
                vec![ContentPart::from_text(format!(
                    "[E1] {}. Answer with citations.",
                    hit.description
                ))],
                None,
            )
            .await
            .unwrap();
            assert!(answer.contains("[E1]"));
            let input = workflow::parts(&source, 0.0, 0.0, String::new())
                .await
                .unwrap();
            let vector = client::embed_media_with_key(&embedding, input, None)
                .await
                .unwrap();
            put_vector(&mut c, &source, &embedding, 0, 0.0, 0.0, 0.0, vector).unwrap();
            let query =
                client::embed_multimodal_query_with_key(&embedding, "绿色画面".into(), None)
                    .await
                    .unwrap();
            assert!(
                (search(&c, &embedding, &query, 10, None, 0).unwrap()[0].score - 1.0).abs() < 1e-6
            );
            assert_ne!(
                embedding.embedding_profile().unwrap(),
                embedding.multimodal_profile().unwrap()
            );
            let seen = worker.join().unwrap();
            assert!(seen[0].to_string().contains("image_url"));
            assert!(seen[1].to_string().contains("绿色照片"));
            assert!(seen[2].to_string().contains("绿色图片"));
            assert!(seen[4].to_string().contains("data:image/jpeg;base64,"));
            if protocol == EmbeddingProtocol::ArkMultimodal {
                assert!(
                    seen[2]["instructions"]
                        .as_str()
                        .unwrap()
                        .starts_with("Target_modality: text.\n")
                );
                assert!(
                    seen[5]["instructions"]
                        .as_str()
                        .unwrap()
                        .starts_with("Target_modality: image/text and image.\n")
                );
            }
            assert_eq!(original, blake3::hash(&std::fs::read(path).unwrap()));
        }
    }

    #[test]
    fn original_vectors_need_no_analysis_and_preserve_models_sources_and_visibility() {
        let (mut c, dir) = store::tests::fixture();
        let src = store::source(&c, 1).unwrap();
        let model = model();
        put_vector(&mut c, &src, &model, 0, 0.0, 0.0, 0.0, vec![3.0, 4.0]).unwrap();
        assert!(store::analyses(&c, &src).unwrap().is_empty());
        assert!(complete(&src, &vectors(&c, &src, &model).unwrap()));
        assert!(
            store::search(&c, &model, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        let mut other = model.clone();
        other.model.revision = "new weights".into();
        assert!(
            search(&c, &other, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        assert!(
            search(&c, &model, &[0.6, 0.8], 10, None, 2)
                .unwrap()
                .is_empty()
        );
        let db = dir.path().join("visual.db");
        c.execute("VACUUM INTO ?1", [db.to_str().unwrap()]).unwrap();
        drop(c);
        let mut c = Connection::open(db).unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        assert!(
            (search(&c, &model, &[0.6, 0.8], 10, None, 0).unwrap()[0].score - 1.0).abs() < 1e-6
        );
        c.execute("UPDATE afiles SET size=size+1 WHERE id=1", [])
            .unwrap();
        assert!(
            search(&c, &model, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        assert!(put_vector(&mut c, &src, &model, 0, 0.0, 0.0, 0.0, vec![1.0, 2.0]).is_err());
        c.execute("UPDATE afiles SET size=?1 WHERE id=1", [src.size])
            .unwrap();
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(
            search(&c, &model, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        assert!(put_vector(&mut c, &src, &model, 0, 0.0, 0.0, 0.0, vec![1.0, 2.0]).is_err());
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM media_ai_visual_vectors", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn text_and_original_search_limit_distinct_media_before_video_segments() {
        let (c, _dir) = store::tests::fixture();
        c.execute_batch("DELETE FROM afiles WHERE id<>1; UPDATE afiles SET file_type=2,duration=6000 WHERE id=1;
            WITH RECURSIVE ids(id) AS(SELECT 2 UNION ALL SELECT id+1 FROM ids WHERE id<51)
            INSERT INTO afiles(id,folder_id,name,file_type,modified_at,size,width,height,e_orientation)
            SELECT id,1,'photo-'||id||'.jpg',1,10,20,1000,1000,1 FROM ids;").unwrap();
        let text_model = store::tests::model("text-embedding");
        let visual_model = model();
        let text_profile = text_model.embedding_profile().unwrap();
        let visual_profile = visual_model.multimodal_profile().unwrap();
        for profile in [&text_profile, &visual_profile] {
            c.execute("INSERT INTO media_ai_profiles VALUES(?1,'{}',1)", [profile])
                .unwrap();
        }
        for id in 1..=51 {
            let src = store::source(&c, id).unwrap();
            let snapshot = serde_json::to_string(&src).unwrap();
            for segment in 0..if id == 1 { 100 } else { 1 } {
                let score = if id == 1 {
                    0.99 + segment as f32 * 0.00005
                } else {
                    0.9 - id as f32 * 0.001
                };
                let vector = [score, (1.0 - score * score).sqrt()]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>();
                let start = if id == 1 { segment as f64 * 60.0 } else { 0.0 };
                let end = if id == 1 { start + 60.0 } else { 0.0 };
                let evidence = format!("{id}-{segment:03}");
                c.execute("INSERT INTO media_ai_analyses(id,file_id,stamp,source,segment,start,end,title,description,tags,model,created_at)
                    VALUES(?1,?2,?3,?4,?5,?6,?7,'title','description','[]','{}',1)",
                    params![evidence,id,src.stamp(),snapshot,segment,start,end]).unwrap();
                c.execute(
                    "INSERT INTO media_ai_vectors VALUES(?1,?2,?3)",
                    params![evidence, text_profile, vector],
                )
                .unwrap();
                c.execute("INSERT INTO media_ai_visual_vectors(id,file_id,stamp,source,segment,start,end,duration,profile_id,vector,created_at)
                    VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1)",
                    params![evidence,id,src.stamp(),snapshot,segment,start,end,if id == 1 {6000.0} else {0.0},visual_profile,vector]).unwrap();
            }
        }
        for hits in [
            store::search(&c, &text_model, &[1.0, 0.0], 100, None, 0).unwrap(),
            search(&c, &visual_model, &[1.0, 0.0], 100, None, 0).unwrap(),
        ] {
            assert_eq!(hits.len(), 51);
            assert_eq!(
                hits.iter()
                    .map(|hit| hit.file_id)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                51
            );
            assert_eq!(hits[0].file_id, 1);
            assert_eq!((hits[0].start, hits[0].end), (5940.0, 6000.0));
        }
        let limited = search(&c, &visual_model, &[1.0, 0.0], 20, None, 0).unwrap();
        assert_eq!(limited.len(), 20);
        assert_eq!(limited.last().unwrap().file_id, 20);
    }

    #[test]
    fn video_vectors_keep_time_ranges_and_resume_without_duplicate_segments() {
        let (mut c, _dir) = store::tests::fixture();
        c.execute("UPDATE afiles SET file_type=2,duration=65 WHERE id=1", [])
            .unwrap();
        let src = store::source(&c, 1).unwrap();
        let model = model();
        put_vector(&mut c, &src, &model, 0, 0.0, 60.0, 65.0, vec![1.0, 0.0]).unwrap();
        assert!(!complete(&src, &vectors(&c, &src, &model).unwrap()));
        put_vector(&mut c, &src, &model, 1, 60.0, 65.0, 65.0, vec![0.0, 1.0]).unwrap();
        put_vector(&mut c, &src, &model, 1, 60.0, 65.0, 65.0, vec![0.0, 2.0]).unwrap();
        assert!(complete(&src, &vectors(&c, &src, &model).unwrap()));
        let hits = search(&c, &model, &[0.0, 1.0], 10, None, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].start, hits[0].end), (60.0, 65.0));
        assert!(put_vector(&mut c, &src, &model, 1, 0.0, 60.0, 65.0, vec![1.0, 0.0]).is_err());
        let mut unprobed = src.clone();
        unprobed.duration = 0;
        assert!(complete(
            &unprobed,
            &vectors(&c, &unprobed, &model).unwrap()
        ));
    }
}

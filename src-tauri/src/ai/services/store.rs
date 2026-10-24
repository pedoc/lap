use super::{client, config::Resolved};
use crate::{ai::media::Source, t_sqlite::AFile};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    time::Duration,
};

pub fn db<T>(
    library: &str,
    f: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    crate::t_cmds::with_current_library(library, || db_current(library, f))
}
fn db_current<T>(
    library: &str,
    f: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let _lease = crate::t_storage::DbConnectionLease::acquire()?;
    let config = crate::t_config::load_app_config()?;
    if config.current_library_id != library
        || !config.libraries.iter().any(|l| l.id == library)
        || library.is_empty()
        || !library
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("图库已切换或不可用，请回到原图库重试。".into());
    }
    let path = crate::t_storage::get_library_db_path(library)?;
    let mut conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|_| "无法打开图库数据库")?;
    conn.busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(sql_error)?;
    schema(&conn)?;
    f(&mut conn)
}
pub fn sql_error(e: rusqlite::Error) -> String {
    format!("媒体理解数据库操作失败：{e}")
}
pub fn schema(c: &Connection) -> Result<(), String> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS media_ai_analyses(
      id TEXT PRIMARY KEY,file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
      stamp TEXT NOT NULL,source TEXT NOT NULL,segment INTEGER NOT NULL,start REAL NOT NULL,end REAL NOT NULL,
      title TEXT NOT NULL,description TEXT NOT NULL,tags TEXT NOT NULL,model TEXT NOT NULL,created_at INTEGER NOT NULL,
      dismissed TEXT NOT NULL DEFAULT '[]',UNIQUE(file_id,stamp,segment,model));
      CREATE INDEX IF NOT EXISTS media_ai_analysis_file ON media_ai_analyses(file_id,created_at);
      CREATE TABLE IF NOT EXISTS media_ai_profiles(id TEXT PRIMARY KEY,snapshot TEXT NOT NULL,created_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS media_ai_vectors(
        analysis_id TEXT NOT NULL REFERENCES media_ai_analyses(id) ON DELETE CASCADE,
        profile_id TEXT NOT NULL REFERENCES media_ai_profiles(id) ON DELETE CASCADE,vector BLOB NOT NULL,
        PRIMARY KEY(analysis_id,profile_id));
      CREATE INDEX IF NOT EXISTS media_ai_vector_profile ON media_ai_vectors(profile_id);
      CREATE TABLE IF NOT EXISTS media_ai_jobs(id TEXT PRIMARY KEY,state TEXT NOT NULL,snapshot TEXT NOT NULL,created_at INTEGER NOT NULL,message TEXT NOT NULL DEFAULT '');
      CREATE TABLE IF NOT EXISTS media_ai_job_items(job_id TEXT NOT NULL REFERENCES media_ai_jobs(id) ON DELETE CASCADE,
        file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,source TEXT NOT NULL,state TEXT NOT NULL DEFAULT 'pending',
        message TEXT NOT NULL DEFAULT '',PRIMARY KEY(job_id,file_id));
      CREATE TABLE IF NOT EXISTS media_ai_answers(id TEXT PRIMARY KEY,file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
        stamp TEXT NOT NULL,question TEXT NOT NULL,answer TEXT NOT NULL,evidence TEXT NOT NULL,created_at INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS media_ai_dismissals(file_id INTEGER NOT NULL REFERENCES afiles(id) ON DELETE CASCADE,
        stamp TEXT NOT NULL,field TEXT NOT NULL,PRIMARY KEY(file_id,stamp,field));")
        .map_err(sql_error)
}
pub fn point_base() -> String {
    format!(
        "FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,2,3) AND {} AND {} AND {}",
        AFile::search_exclusion_condition("b"),
        AFile::live_photo_companion_point_condition(),
        AFile::album_filter_predicate("a")
    )
}
pub fn bulk_base() -> String {
    format!(
        "FROM afiles a JOIN afolders b ON b.id=a.folder_id WHERE a.file_type IN (1,2,3) AND {}",
        crate::ai::media::visible()
    )
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaSource {
    pub file_id: i64,
    pub path: String,
    pub modified: Option<i64>,
    pub size: i64,
    pub orientation: i32,
    pub kind: i64,
    pub width: u64,
    pub height: u64,
    pub duration: i64,
    pub title: Option<String>,
    pub comments: Option<String>,
    pub tag_ids: Vec<i64>,
    pub cloud_only: bool,
}
impl MediaSource {
    pub fn stamp(&self) -> String {
        blake3::hash(
            serde_json::json!([
                self.path,
                self.modified,
                self.size,
                self.orientation,
                self.kind
            ])
            .to_string()
            .as_bytes(),
        )
        .to_hex()
        .to_string()
    }
    pub fn verify(&self) -> Result<(), String> {
        if self.cloud_only || crate::t_utils::is_cloud_only_path(&self.path) {
            return Err("原文件仅在云端，请下载到本机后处理。".into());
        }
        let info = crate::t_utils::FileInfo::new(&self.path)?;
        if info.modified != self.modified || info.file_size != self.size {
            return Err("原文件已变化，请重新扫描后重试。".into());
        }
        Ok(())
    }
    pub fn image(&self) -> Result<image::DynamicImage, String> {
        self.verify()?;
        Source {
            path: self.path.clone(),
            modified: self.modified,
            size: self.size,
            width: self.width,
            height: self.height,
            orientation: self.orientation,
        }
        .load()
    }
}
pub fn source(c: &Connection, id: i64) -> Result<MediaSource, String> {
    let mut source=c.query_row(&format!("SELECT a.id,b.path||'/'||a.name,a.modified_at,a.size,COALESCE(a.e_orientation,1),a.file_type,COALESCE(a.width,0),COALESCE(a.height,0),COALESCE(a.duration,0),a.title,a.comments,COALESCE(a.cloud_only,0) {} AND a.id=?1",point_base()),[id],|r|Ok(MediaSource{
        file_id:r.get(0)?,path:r.get(1)?,modified:r.get(2)?,size:r.get(3)?,orientation:r.get(4)?,kind:r.get(5)?,width:r.get(6)?,height:r.get(7)?,duration:r.get(8)?,title:r.get(9)?,comments:r.get(10)?,tag_ids:Vec::new(),cloud_only:r.get(11)?,
    })).optional().map_err(sql_error)?.ok_or("媒体不可用、被隐藏、已删除或不在可见图库内")?;
    source.tag_ids = c
        .prepare("SELECT tag_id FROM afile_tags WHERE file_id=?1 ORDER BY tag_id")
        .map_err(sql_error)?
        .query_map([id], |r| r.get(0))
        .map_err(sql_error)?
        .collect::<Result<_, _>>()
        .map_err(sql_error)?;
    Ok(source)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisText {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
}
impl AnalysisText {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let text = if let Some(rest) = text
            .strip_prefix("```json")
            .or_else(|| text.strip_prefix("```"))
        {
            rest.trim()
                .strip_suffix("```")
                .ok_or("模型 JSON 代码块未结束")?
                .trim()
        } else {
            text
        };
        let value: Self = serde_json::from_str(text).map_err(
            |_| "模型未返回有效的 title / description / tags JSON；分析未保存，可重试。",
        )?;
        if value.title.trim().is_empty()
            || value.title.chars().count() > 256
            || value.description.trim().is_empty()
            || value.description.chars().count() > 8000
            || value.tags.len() > 20
            || value.title.chars().any(char::is_control)
            || value.tags.iter().any(|s| {
                s.trim().is_empty() || s.chars().count() > 64 || s.chars().any(char::is_control)
            })
        {
            return Err("模型输出字段为空、类型错误或超过长度限制；分析未保存。".into());
        }
        let mut tags = Vec::new();
        for tag in value.tags {
            let tag = tag.trim().to_owned();
            if !tags
                .iter()
                .any(|old: &String| old.eq_ignore_ascii_case(&tag))
            {
                tags.push(tag);
            }
        }
        Ok(Self {
            title: value.title.trim().into(),
            description: value.description.trim().into(),
            tags,
        })
    }
    pub fn document(&self) -> String {
        format!(
            "{}\n{}\n标签：{}",
            self.title,
            self.description,
            self.tags.join("、")
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub id: String,
    pub file_id: i64,
    pub stamp: String,
    pub segment: i64,
    pub start: f64,
    pub end: f64,
    #[serde(flatten)]
    pub text: AnalysisText,
    pub model: Resolved,
    pub created_at: i64,
    pub dismissed: Vec<String>,
    pub indexed: bool,
}
pub fn analyses(c: &Connection, src: &MediaSource) -> Result<Vec<Analysis>, String> {
    let mut statement=c.prepare("SELECT id,segment,start,end,title,description,tags,model,created_at,(SELECT json_group_array(field) FROM media_ai_dismissals d WHERE d.file_id=media_ai_analyses.file_id AND d.stamp=media_ai_analyses.stamp),EXISTS(SELECT 1 FROM media_ai_vectors v WHERE v.analysis_id=media_ai_analyses.id) FROM media_ai_analyses WHERE file_id=?1 AND stamp=?2 ORDER BY segment,created_at DESC").map_err(sql_error)?;
    let rows = statement
        .query_map(params![src.file_id, src.stamp()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, f64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, String>(9)?,
                r.get::<_, bool>(10)?,
            ))
        })
        .map_err(sql_error)?;
    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            segment,
            start,
            end,
            title,
            description,
            tags,
            model,
            created_at,
            dismissed,
            indexed,
        ) = row.map_err(sql_error)?;
        // The latest model result for each segment is active; older versions remain recoverable.
        if result.iter().any(|a: &Analysis| a.segment == segment) {
            continue;
        }
        result.push(Analysis {
            id,
            file_id: src.file_id,
            stamp: src.stamp(),
            segment,
            start,
            end,
            text: AnalysisText {
                title,
                description,
                tags: serde_json::from_str(&tags).map_err(|_| "标签记录损坏")?,
            },
            model: serde_json::from_str(&model).map_err(|_| "分析模型快照损坏")?,
            created_at,
            dismissed: serde_json::from_str(&dismissed).map_err(|_| "建议状态损坏")?,
            indexed,
        });
    }
    Ok(result)
}
pub fn put_analysis(
    c: &mut Connection,
    src: &MediaSource,
    segment: i64,
    start: f64,
    end: f64,
    text: AnalysisText,
    model: &Resolved,
) -> Result<(), String> {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    if source(&tx, src.file_id)?.stamp() != src.stamp() {
        return Err("媒体或可见性已变化，结果未写入".into());
    }
    src.verify()?;
    let previous: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(created_at),0) FROM media_ai_analyses WHERE file_id=?1",
            [src.file_id],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    let created_at = chrono::Utc::now()
        .timestamp_millis()
        .max(previous.saturating_add(1));
    // Replace a repeated attempt transactionally; vectors cascade, unrelated model history stays.
    let snapshot = serde_json::to_string(model).map_err(|_| "模型快照无效")?;
    tx.execute(
        "DELETE FROM media_ai_analyses WHERE file_id=?1 AND stamp=?2 AND segment=?3 AND model=?4",
        params![src.file_id, src.stamp(), segment, snapshot],
    )
    .map_err(sql_error)?;
    tx.execute("INSERT INTO media_ai_analyses(id,file_id,stamp,source,segment,start,end,title,description,tags,model,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![uuid::Uuid::new_v4().to_string(),src.file_id,src.stamp(),serde_json::to_string(src).map_err(|_|"来源快照无效")?,segment,start,end,text.title,text.description,serde_json::to_string(&text.tags).map_err(|_|"标签无效")?,snapshot,created_at]).map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}
pub fn put_vector(
    c: &Connection,
    analysis: &Analysis,
    model: &Resolved,
    vector: Vec<f32>,
) -> Result<(), String> {
    if source(c, analysis.file_id)?.stamp() != analysis.stamp {
        return Err("媒体已变化，向量未写入".into());
    }
    let vector = client::normalize(vector, model.model.embedding.dimensions)?;
    let profile = model.embedding_profile()?;
    let tx = c.unchecked_transaction().map_err(sql_error)?;
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
    tx.execute(
        "INSERT OR REPLACE INTO media_ai_vectors VALUES(?1,?2,?3)",
        params![analysis.id, profile, bytes],
    )
    .map_err(sql_error)?;
    tx.commit().map_err(sql_error)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub file_id: i64,
    pub analysis_id: String,
    pub title: String,
    pub description: String,
    pub start: f64,
    pub end: f64,
    pub score: f32,
}
impl PartialEq for Hit {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.analysis_id == other.analysis_id
    }
}
impl Eq for Hit {}
impl PartialOrd for Hit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Hit {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| self.analysis_id.cmp(&other.analysis_id))
    }
}
pub fn search(
    c: &Connection,
    model: &Resolved,
    query: &[f32],
    limit: usize,
    album_id: Option<i64>,
    file_type: i64,
) -> Result<Vec<Hit>, String> {
    // shortcut: exact cosine streams SQLite vectors with bounded memory; add ANN after measured large-library latency warrants it.
    let sql=format!("SELECT a.id,x.id,x.title,x.description,x.start,x.end,v.vector,x.source FROM media_ai_vectors v JOIN media_ai_analyses x ON x.id=v.analysis_id JOIN afiles a ON a.id=x.file_id JOIN afolders b ON b.id=a.folder_id
      WHERE v.profile_id=?1 AND (?2 IS NULL OR b.album_id=?2) AND (?3=0 OR (?3 & CASE a.file_type WHEN 1 THEN 1 WHEN 2 THEN 2 WHEN 3 THEN 4 ELSE 0 END)!=0) AND {} AND a.file_type IN(1,2,3)
      AND NOT EXISTS(SELECT 1 FROM media_ai_analyses newer WHERE newer.file_id=x.file_id AND newer.stamp=x.stamp AND newer.segment=x.segment AND newer.created_at>x.created_at)
      AND json_extract(x.source,'$.kind')=a.file_type AND json_extract(x.source,'$.path')=b.path||'/'||a.name AND json_extract(x.source,'$.size')=a.size AND json_extract(x.source,'$.modified') IS a.modified_at AND json_extract(x.source,'$.orientation')=COALESCE(a.e_orientation,1)",crate::ai::media::visible());
    let mut stmt = c.prepare(&sql).map_err(sql_error)?;
    let rows = stmt
        .query_map(
            params![model.embedding_profile()?, album_id, file_type],
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
    let mut heap = BinaryHeap::new();
    let limit = limit.clamp(1, 200);
    for row in rows {
        let (mut hit, bytes) = row.map_err(sql_error)?;
        if bytes.len() != query.len() * 4 {
            continue;
        }
        let vector: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        if vector.iter().any(|v| !v.is_finite()) {
            continue;
        }
        hit.score = vector.iter().zip(query).map(|(v, q)| v * q).sum();
        if heap.len() < limit {
            heap.push(Reverse(hit));
        } else if heap.peek().is_some_and(|min: &Reverse<Hit>| hit > min.0) {
            heap.pop();
            heap.push(Reverse(hit));
        }
    }
    let mut hits: Vec<_> = heap.into_iter().map(|r| r.0).collect();
    hits.sort_by(|a, b| b.cmp(a));
    Ok(hits)
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Adoption {
    pub title: bool,
    pub description: bool,
    pub tags: bool,
    pub create_tags: bool,
    pub only_empty: bool,
}
pub fn adopt(
    c: &mut Connection,
    src: &MediaSource,
    analysis_id: &str,
    options: &Adoption,
) -> Result<(), String> {
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql_error)?;
    let current = source(&tx, src.file_id)?;
    if current.stamp() != src.stamp() {
        return Err("媒体已变化，请重新理解".into());
    }
    let analysis = analyses(&tx, &current)?
        .into_iter()
        .find(|a| a.id == analysis_id)
        .ok_or("建议已失效，请刷新")?;
    if options.title && current.title != src.title
        || options.description && current.comments != src.comments
        || options.tags && current.tag_ids != src.tag_ids
    {
        return Err("资料已被其他操作修改，请刷新后重新采用。".into());
    }
    if options.title
        && !analysis.dismissed.contains(&"title".into())
        && (!options.only_empty
            || current
                .title
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty())
    {
        crate::t_media_title::set_on(
            &tx,
            src.file_id,
            Some(&analysis.text.title),
            src.title.as_deref(),
        )?;
    }
    if options.description
        && !analysis.dismissed.contains(&"description".into())
        && (!options.only_empty
            || current
                .comments
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty())
    {
        tx.execute(
            "UPDATE afiles SET comments=?1 WHERE id=?2",
            params![analysis.text.description, src.file_id],
        )
        .map_err(sql_error)?;
    }
    if options.tags && !analysis.dismissed.contains(&"tags".into()) {
        for name in &analysis.text.tags {
            let mut tag: Option<i64> = tx
                .query_row(
                    "SELECT id FROM atags WHERE name=?1 COLLATE NOCASE",
                    [name],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if tag.is_none() && options.create_tags {
                tx.execute("INSERT INTO atags(name,group_id) SELECT ?1,id FROM atag_groups WHERE is_default=1",[name]).map_err(sql_error)?;
                tag = tx
                    .query_row(
                        "SELECT id FROM atags WHERE name=?1 COLLATE NOCASE",
                        [name],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
            }
            if let Some(tag) = tag {
                tx.execute(
                    "INSERT OR IGNORE INTO afile_tags(file_id,tag_id) VALUES(?1,?2)",
                    params![src.file_id, tag],
                )
                .map_err(sql_error)?;
            }
        }
        tx.execute("UPDATE afiles SET has_tags=EXISTS(SELECT 1 FROM afile_tags WHERE file_id=?1) WHERE id=?1",[src.file_id]).map_err(sql_error)?;
    }
    tx.commit().map_err(sql_error)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture() -> (Connection, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo-1.jpg");
        std::fs::write(&path, b"unchanged original").unwrap();
        let c = crate::ai::media::tests::db(3);
        c.execute_batch("ALTER TABLE afiles ADD COLUMN title TEXT;ALTER TABLE afiles ADD COLUMN title_sort_key TEXT;ALTER TABLE afiles ADD COLUMN comments TEXT;ALTER TABLE afiles ADD COLUMN duration INTEGER;ALTER TABLE afiles ADD COLUMN has_tags INTEGER DEFAULT 0;
          CREATE TABLE atag_groups(id INTEGER PRIMARY KEY,is_default INTEGER);INSERT INTO atag_groups VALUES(1,1);
          CREATE TABLE atags(id INTEGER PRIMARY KEY,name TEXT UNIQUE,group_id INTEGER);INSERT INTO atags VALUES(1,'湖泊',1),(2,'人工标签',1);
          CREATE TABLE afile_tags(file_id INTEGER,tag_id INTEGER,PRIMARY KEY(file_id,tag_id));PRAGMA foreign_keys=ON;").unwrap();
        let info = crate::t_utils::FileInfo::new(path.to_str().unwrap()).unwrap();
        c.execute(
            "UPDATE afolders SET path=?1 WHERE id=1",
            [dir.path().to_string_lossy().replace('\\', "/")],
        )
        .unwrap();
        c.execute(
            "UPDATE afiles SET size=?1,modified_at=?2 WHERE id=1",
            params![info.file_size, info.modified],
        )
        .unwrap();
        schema(&c).unwrap();
        (c, dir)
    }
    pub(crate) fn model(id: &str) -> Resolved {
        Resolved {
            provider: super::super::config::Provider {
                id: "provider".into(),
                base_url: "http://127.0.0.1:8002/v1/".into(),
                ..Default::default()
            },
            model: super::super::config::Model {
                id: id.into(),
                provider_id: "provider".into(),
                remote_id: id.into(),
                embedding: super::super::config::EmbeddingOptions {
                    dimensions: Some(2),
                    query_instruction: String::new(),
                },
                ..Default::default()
            },
        }
    }
    fn seed(c: &mut Connection) -> (MediaSource, Analysis) {
        let src = source(c, 1).unwrap();
        put_analysis(
            c,
            &src,
            0,
            0.0,
            0.0,
            AnalysisText {
                title: "湖边散步".into(),
                description: "有人在湖边散步".into(),
                tags: vec!["湖泊".into(), "新建议".into()],
            },
            &model("generator"),
        )
        .unwrap();
        let a = analyses(c, &src).unwrap().remove(0);
        (src, a)
    }
    #[test]
    fn output_validation_does_not_accept_partial_or_oversized_json() {
        assert!(AnalysisText::parse(r#"{"title":"","description":"text","tags":[]}"#).is_err());
        assert!(
            AnalysisText::parse(r#"{"title":"ok","description":"text","tags":"tag"}"#).is_err()
        );
        assert!(
            AnalysisText::parse(
                "{\"title\":\"ok\",\"description\":\"text\",\"tags\":[\"bad\\n\"]}"
            )
            .is_err()
        );
        let parsed=AnalysisText::parse("```json\n{\"title\":\"ok\",\"description\":\"text\",\"tags\":[\" Lake \",\"lake\"]}\n```").unwrap();
        assert_eq!(parsed.tags, vec!["Lake"]);
        assert_eq!(parsed.document(), "ok\ntext\n标签：Lake");
    }
    #[test]
    fn analysis_never_writes_catalogue_and_adoption_preserves_manual_changes() {
        let (mut c, _dir) = fixture();
        let (src, a) = seed(&mut c);
        assert!(source(&c, 1).unwrap().title.is_none());
        c.execute("UPDATE afiles SET comments='人工说明' WHERE id=1", [])
            .unwrap();
        assert!(
            adopt(
                &mut c,
                &src,
                &a.id,
                &Adoption {
                    title: true,
                    description: true,
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            source(&c, 1).unwrap().title.is_none(),
            "conflict rolls back every field"
        );
        let fresh = source(&c, 1).unwrap();
        adopt(
            &mut c,
            &fresh,
            &a.id,
            &Adoption {
                title: true,
                description: true,
                tags: true,
                only_empty: true,
                ..Default::default()
            },
        )
        .unwrap();
        let updated = source(&c, 1).unwrap();
        assert_eq!(updated.title.as_deref(), Some("湖边散步"));
        assert_eq!(updated.comments.as_deref(), Some("人工说明"));
        assert_eq!(updated.tag_ids, vec![1]);
        let key: Option<String> = c
            .query_row("SELECT title_sort_key FROM afiles WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(key.is_some());
        assert_eq!(std::fs::read(&src.path).unwrap(), b"unchanged original");
    }
    #[test]
    fn vector_spaces_survive_reopen_and_exclude_stale_hidden_and_deleted_media() {
        let (mut c, dir) = fixture();
        let (src, a) = seed(&mut c);
        let embedding = model("embedding-a");
        put_vector(&c, &a, &embedding, vec![3.0, 4.0]).unwrap();
        assert!(
            search(&c, &model("embedding-b"), &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        assert!(
            search(&c, &embedding, &[0.6, 0.8], 10, None, 2)
                .unwrap()
                .is_empty()
        );
        let path = dir.path().join("persisted.db");
        c.execute("VACUUM INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        drop(c);
        let c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let hits = search(&c, &embedding, &[0.6, 0.8], 10, None, 0).unwrap();
        assert_eq!(hits[0].file_id, 1);
        assert!((hits[0].score - 1.0).abs() < 1e-6);
        c.execute("UPDATE afiles SET size=size+1 WHERE id=1", [])
            .unwrap();
        assert!(
            search(&c, &embedding, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        c.execute("UPDATE afiles SET size=?1 WHERE id=1", [src.size])
            .unwrap();
        c.execute("INSERT INTO media_states VALUES(1,1,NULL)", [])
            .unwrap();
        assert!(
            search(&c, &embedding, &[0.6, 0.8], 10, None, 0)
                .unwrap()
                .is_empty()
        );
        c.execute("DELETE FROM media_states", []).unwrap();
        c.execute("DELETE FROM afiles WHERE id=1", []).unwrap();
        assert_eq!(
            c.query_row("SELECT COUNT(*) FROM media_ai_vectors", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn source_version_dismissals_survive_reanalysis_and_tags_are_append_only() {
        let (mut c, _dir) = fixture();
        let (src, _) = seed(&mut c);
        c.execute(
            "INSERT INTO media_ai_dismissals VALUES(1,?1,'title')",
            [src.stamp()],
        )
        .unwrap();
        seed(&mut c);
        let a = analyses(&c, &src).unwrap().remove(0);
        assert!(a.dismissed.contains(&"title".into()));
        adopt(
            &mut c,
            &src,
            &a.id,
            &Adoption {
                title: true,
                tags: true,
                create_tags: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(source(&c, 1).unwrap().title.is_none());
        assert_eq!(source(&c, 1).unwrap().tag_ids.len(), 2);
        assert!(
            adopt(
                &mut c,
                &src,
                &a.id,
                &Adoption {
                    tags: true,
                    ..Default::default()
                }
            )
            .is_err(),
            "stale tag snapshot is rejected"
        );
    }
}

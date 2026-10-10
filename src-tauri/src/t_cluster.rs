use crate::t_sqlite::{Face, Person};
use rand::seq::SliceRandom;
use std::collections::HashMap;

/// Clustering progress information
#[derive(Clone, serde::Serialize)]
pub struct ClusterProgress {
    pub phase: String, // "graph", "iterate", "converged", "assign", "thumbnail"
    pub current: usize,
    pub total: usize,
}

/// Calculate cosine distance between two PRE-PARSED embeddings
/// Distance = 1.0 - Cosine Similarity
/// NOTE: Assumes input vectors are already normalized!
fn cosine_distance(emb1: &[f32], emb2: &[f32]) -> f32 {
    // Dot product of normalized vectors = cosine similarity
    let dot_product: f32 = emb1.iter().zip(emb2.iter()).map(|(x, y)| x * y).sum();

    // Clamp similarity to [-1.0, 1.0] to handle floating point errors
    let similarity = dot_product.clamp(-1.0, 1.0);
    1.0 - similarity
}

/// Helper: Parse raw byte embedding to normalized f32 vector
fn parse_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    // Embedding bytes should be tightly packed f32 values.
    // If not, skip this embedding instead of panicking.
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let emb_vec: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr = [chunk[0], chunk[1], chunk[2], chunk[3]];
            f32::from_le_bytes(arr)
        })
        .collect();

    // Normalize
    let norm: f32 = emb_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm.is_finite() && norm > 0.0 && emb_vec.iter().all(|value| value.is_finite()) {
        Some(emb_vec.iter().map(|x| x / norm).collect())
    } else {
        None
    }
}

/// Edge in the similarity graph
#[derive(Clone)]
struct Edge {
    to: usize,   // Target node index
    weight: f32, // Edge weight (similarity = 1 - distance)
}

/// Insert edge into candidate list, maintaining at most max_edges entries sorted by weight descending.
/// If list is full and new weight is smaller than the smallest, it is dropped.
fn insert_top_k(candidates: &mut Vec<(usize, f32)>, edge: (usize, f32), max_edges: usize) {
    if candidates.len() < max_edges {
        candidates.push(edge);
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        // Find the smallest weight (last element after sort)
        if let Some(&(_, min_weight)) = candidates.last() {
            if edge.1 > min_weight {
                candidates.pop();
                candidates.push(edge);
                candidates
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
    }
}

/// Run Chinese Whispers clustering on ALL faces
///
/// Memory-optimized:
/// 1. Uses slim face data (id, file_id, embedding_bytes) instead of full Face structs
/// 2. Prunes candidate edges to Top-K during build (not after), bounding memory at N * K_NEIGHBORS
/// 3. Pre-parses all embeddings once to avoid allocations in inner loop
#[derive(Default, Debug)]
pub struct ClusterResult {
    pub total_persons: usize,
    pub person_ids: Vec<i64>,
    pub file_ids: Vec<i64>,
}

pub fn cluster_faces<F, C>(
    threshold: f32,
    file_ids: Option<&[i64]>,
    mut progress_fn: F,
    is_cancelled_fn: C,
) -> Result<ClusterResult, String>
where
    F: FnMut(ClusterProgress),
    C: Fn() -> bool,
{
    let model = crate::ai::settings::active(crate::ai::types::Task::Face)?;
    let conn = crate::t_sqlite::open_conn()?;
    let result = group_faces(
        &conn,
        Face::get_all_for_clustering()?,
        threshold,
        model.number("cluster_neighbors") as usize,
        model.number("cluster_iterations") as usize,
        model.number("cluster_min_samples") as usize,
        file_ids,
        &mut progress_fn,
        &is_cancelled_fn,
    )?;
    progress_fn(ClusterProgress {
        phase: "thumbnail".into(),
        current: 0,
        total: result.person_ids.len(),
    });
    for person in &result.person_ids {
        if is_cancelled_fn() {
            break;
        }
        let _ = Person::update_thumbnail(*person);
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn group_faces<F: FnMut(ClusterProgress), C: Fn() -> bool>(
    conn: &rusqlite::Connection,
    mut slim_faces: Vec<(i64, i64, Option<Vec<u8>>, Option<i64>)>,
    threshold: f32,
    k_neighbors: usize,
    max_iterations: usize,
    min_samples: usize,
    file_ids: Option<&[i64]>,
    mut progress_fn: F,
    is_cancelled_fn: C,
) -> Result<ClusterResult, String> {
    // Existing automatic/manual identities remain immutable anchors across batches.
    if let Some(ids) = file_ids {
        let scope = ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        slim_faces.retain(|face| face.3.is_some() || scope.contains(&face.1));
    }
    let existing = slim_faces.iter().map(|face| face.3).collect::<Vec<_>>();
    let selected = file_ids.map(|ids| {
        ids.iter()
            .copied()
            .collect::<std::collections::HashSet<_>>()
    });
    let mut changed_people = slim_faces
        .iter()
        .filter(|face| selected.as_ref().is_some_and(|ids| ids.contains(&face.1)))
        .filter_map(|face| face.3)
        .collect::<std::collections::HashSet<_>>();
    let (mut labels, anchor_people) = seed_people(&existing);
    let n = slim_faces.len();
    if n == 0 {
        return Ok(ClusterResult::default());
    }

    // 3. Pre-parse embeddings (do this once)
    let mut parsed_embeddings: Vec<Option<Vec<f32>>> = Vec::with_capacity(n);
    for (_id, _file_id, embedding_bytes, _person_id) in &mut slim_faces {
        parsed_embeddings.push(embedding_bytes.as_deref().and_then(parse_embedding));
        embedding_bytes.take();
    }

    // 4. Build K-NN Graph with early Top-K pruning
    //    candidate_lists memory is bounded at N * K_NEIGHBORS entries
    let mut candidate_lists: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    let pending = existing.iter().filter(|id| id.is_none()).count();
    let total_pairs =
        pending.saturating_mul(n - pending) + pending.saturating_mul(pending.saturating_sub(1)) / 2;
    let mut pairs_done: usize = 0;
    let mut last_pct = 0;

    for i in 0..n {
        // Check for cancellation
        if is_cancelled_fn() {
            return Ok(ClusterResult::default());
        }

        if existing[i].is_some() {
            continue;
        }
        if let Some(emb_i) = &parsed_embeddings[i] {
            for j in 0..n {
                if i == j || (existing[j].is_none() && j < i) {
                    continue;
                }
                if let Some(emb_j) = &parsed_embeddings[j] {
                    // Faces in the same file cannot be edges (prevents merging distinct people in same photo)
                    if slim_faces[i].1 == slim_faces[j].1 {
                        pairs_done += 1;
                        continue;
                    }

                    let dist = cosine_distance(emb_i, emb_j);

                    if dist < threshold {
                        let weight = 1.0 - dist;
                        // Square weight to punish weak links further
                        let adjusted_weight = weight * weight;

                        insert_top_k(&mut candidate_lists[i], (j, adjusted_weight), k_neighbors);
                        insert_top_k(&mut candidate_lists[j], (i, adjusted_weight), k_neighbors);
                    }
                }
                pairs_done += 1;
            }
        } else {
            pairs_done += n.saturating_sub(1);
        }

        // Report progress every 5%
        let current_pct = if total_pairs > 0 {
            (pairs_done.saturating_mul(100) / total_pairs).min(100)
        } else {
            100
        };
        if current_pct >= last_pct + 5 || pairs_done == total_pairs {
            progress_fn(ClusterProgress {
                phase: "graph".to_string(),
                current: current_pct,
                total: 100,
            });
            last_pct = current_pct;
        }
    }

    // 5. Build final graph from pruned candidate lists (edges already Top-K)
    let mut graph: Vec<Vec<Edge>> = vec![Vec::new(); n];

    for (i, candidates) in candidate_lists.iter().enumerate() {
        for &(to, weight) in candidates {
            graph[i].push(Edge { to, weight });
        }
    }

    // Free graph-building allocations before Chinese Whispers
    drop(candidate_lists);
    drop(parsed_embeddings);

    // 6. Run Chinese Whispers Algorithm
    let mut order: Vec<usize> = (0..n).collect();
    let mut rng = rand::thread_rng();

    for iter in 0..max_iterations {
        // Check for cancellation
        if is_cancelled_fn() {
            return Ok(ClusterResult::default());
        }

        let mut changed = false;

        progress_fn(ClusterProgress {
            phase: "iterate".to_string(),
            current: iter + 1,
            total: max_iterations,
        });

        order.shuffle(&mut rng);

        for &node in &order {
            if existing[node].is_some() || graph[node].is_empty() {
                continue;
            }

            // Count weighted votes
            let mut label_weights: HashMap<usize, f32> = HashMap::new();
            for edge in &graph[node] {
                let neighbor_label = labels[edge.to];
                *label_weights.entry(neighbor_label).or_insert(0.0) += edge.weight;
            }

            // Find best label
            let best_label = label_weights
                .into_iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(label, _)| label)
                .unwrap_or(labels[node]);

            if labels[node] != best_label {
                labels[node] = best_label;
                changed = true;
            }
        }

        if !changed {
            progress_fn(ClusterProgress {
                phase: "converged".to_string(),
                current: iter + 1,
                total: max_iterations,
            });
            break;
        }
    }

    // Check for cancellation before assignment
    if is_cancelled_fn() {
        return Ok(ClusterResult::default());
    }

    // 6. Collect clusters
    let mut cluster_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &label) in labels.iter().enumerate() {
        // if !graph[i].is_empty() {
        cluster_map.entry(label).or_default().push(i);
        // }
    }
    drop(graph);
    drop(labels);
    drop(order);

    // 7. Filter clusters
    let valid_clusters: Vec<_> = cluster_map
        .into_iter()
        .filter(|(label, face_indices)| {
            anchor_people.contains_key(label) || face_indices.len() >= min_samples
        })
        .collect();

    let total_clusters = valid_clusters.len();

    // 8. Assign faces to persons
    let mut changed_files = std::collections::HashSet::new();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let assignment = (|| -> Result<(), String> {
        for (cluster_idx, (label, cluster_face_indices)) in valid_clusters.into_iter().enumerate() {
            if is_cancelled_fn() {
                return Err("Face grouping cancelled".into());
            }

            progress_fn(ClusterProgress {
                phase: "assign".to_string(),
                current: cluster_idx + 1,
                total: total_clusters,
            });

            let person_id = match anchor_people.get(&label) {
                Some(id) => *id,
                None => {
                    conn.execute(
                        "INSERT INTO persons(name,created_at) VALUES(NULL,?1)",
                        [chrono::Utc::now().timestamp()],
                    )
                    .map_err(|e| e.to_string())?;
                    conn.last_insert_rowid()
                }
            };

            for face_idx in cluster_face_indices {
                if is_cancelled_fn() {
                    return Err("Face grouping cancelled".into());
                }
                if existing[face_idx].is_some() {
                    continue;
                }
                let changed = conn
                    .execute(
                        "UPDATE faces SET person_id=?1 WHERE id=?2 AND person_id IS NULL",
                        rusqlite::params![person_id, slim_faces[face_idx].0],
                    )
                    .map_err(|e| e.to_string())?;
                if changed != 1 {
                    return Err("Face membership changed during grouping".into());
                }
                changed_people.insert(person_id);
                changed_files.insert(slim_faces[face_idx].1);
            }
        }

        if is_cancelled_fn() {
            return Err("Face grouping cancelled".into());
        }
        Ok(())
    })();
    match assignment {
        Ok(()) => conn.execute_batch("COMMIT").map_err(|e| e.to_string())?,
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            if is_cancelled_fn() {
                return Ok(ClusterResult::default());
            }
            return Err(error);
        }
    }
    drop(slim_faces);

    Ok(ClusterResult {
        total_persons: total_clusters,
        person_ids: changed_people.into_iter().collect(),
        file_ids: changed_files.into_iter().collect(),
    })
}

fn seed_people(existing: &[Option<i64>]) -> (Vec<usize>, HashMap<usize, i64>) {
    let mut by_person = HashMap::new();
    let mut anchors = HashMap::new();
    let labels = existing
        .iter()
        .enumerate()
        .map(|(index, person)| match person {
            Some(person) => {
                let label = *by_person.entry(*person).or_insert(index);
                anchors.insert(label, *person);
                label
            }
            None => index,
        })
        .collect();
    (labels, anchors)
}
#[cfg(test)]
mod face_anchor_tests {
    use super::*;
    fn db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE persons(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT,created_at INTEGER,hidden INTEGER DEFAULT 0,cover_face_id INTEGER); CREATE TABLE faces(id INTEGER PRIMARY KEY,file_id INTEGER,embedding BLOB,person_id INTEGER); INSERT INTO faces(id,file_id) VALUES(1,1),(2,2);").unwrap();
        conn
    }
    fn rows(conn: &rusqlite::Connection) -> Vec<(i64, i64, Option<Vec<u8>>, Option<i64>)> {
        let mut statement = conn
            .prepare("SELECT id,file_id,embedding,person_id FROM faces ORDER BY id")
            .unwrap();
        let mut rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for row in &mut rows {
            row.2 = Some([1.0f32, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect());
        }
        rows
    }
    #[test]
    fn early_batch_creates_people_and_later_batches_reuse_stable_identity() {
        let c = db();
        let first = group_faces(&c, rows(&c), 0.4, 10, 10, 2, None, |_| {}, || false).unwrap();
        assert_eq!(first.person_ids.len(), 1);
        assert_eq!(first.file_ids.len(), 2);
        let person = first.person_ids[0];
        c.execute(
            "UPDATE persons SET name='Alice',hidden=1,cover_face_id=1 WHERE id=?1",
            [person],
        )
        .unwrap();
        c.execute("INSERT INTO faces(id,file_id) VALUES(3,3)", [])
            .unwrap();
        let second =
            group_faces(&c, rows(&c), 0.4, 10, 10, 2, Some(&[3]), |_| {}, || false).unwrap();
        assert_eq!(second.person_ids, vec![person]);
        assert_eq!(second.file_ids, vec![3]);
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=3", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            person
        );
        assert_eq!(
            c.query_row("SELECT name,hidden,cover_face_id FROM persons", [], |r| Ok(
                (
                    r.get::<_, String>(0)?,
                    r.get::<_, bool>(1)?,
                    r.get::<_, i64>(2)?
                )
            ))
            .unwrap(),
            ("Alice".into(), true, 1)
        );
    }
    #[test]
    fn grouping_failure_rolls_back_every_assignment_and_new_person() {
        let c = db();
        c.execute_batch("CREATE TRIGGER fail_second BEFORE UPDATE ON faces WHEN NEW.id=2 BEGIN SELECT RAISE(ABORT,'failed assignment'); END;").unwrap();
        assert!(group_faces(&c, rows(&c), 0.4, 10, 10, 2, None, |_| {}, || false).is_err());
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM faces WHERE person_id IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn cancelling_assignment_does_not_publish_a_partial_batch() {
        let c = db();
        let cancelled = std::cell::Cell::new(false);
        let result = group_faces(
            &c,
            rows(&c),
            0.4,
            10,
            10,
            2,
            None,
            |progress| {
                if progress.phase == "assign" {
                    cancelled.set(true);
                }
            },
            || cancelled.get(),
        )
        .unwrap();
        assert!(result.person_ids.is_empty());
        assert!(result.file_ids.is_empty());
        assert_eq!(
            c.query_row("SELECT count(*) FROM persons", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM faces WHERE person_id IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn unfinished_small_cluster_is_available_to_a_later_batch_and_selection_is_fenced() {
        let c = db();
        c.execute("DELETE FROM faces WHERE id=2", []).unwrap();
        let first = group_faces(&c, rows(&c), 0.4, 10, 10, 2, None, |_| {}, || false).unwrap();
        assert_eq!(first.total_persons, 0);
        c.execute("INSERT INTO faces(id,file_id) VALUES(2,2),(3,3)", [])
            .unwrap();
        let second = group_faces(
            &c,
            rows(&c),
            0.4,
            10,
            10,
            2,
            Some(&[1, 2]),
            |_| {},
            || false,
        )
        .unwrap();
        assert_eq!(second.total_persons, 1);
        assert_eq!(
            c.query_row("SELECT person_id FROM faces WHERE id=3", [], |r| r
                .get::<_, Option<i64>>(0))
                .unwrap(),
            None
        );
    }
    #[test]
    fn existing_people_keep_stable_labels_and_new_faces_do_not_replace_them() {
        let (labels, people) = seed_people(&[Some(7), None, Some(7), Some(9)]);
        assert_eq!(labels, vec![0, 1, 0, 3]);
        assert_eq!(people[&0], 7);
        assert_eq!(people[&3], 9);
    }
    #[test]
    fn all_unknown_faces_start_as_distinct_labels() {
        let (labels, people) = seed_people(&[None, None]);
        assert_eq!(labels, vec![0, 1]);
        assert!(people.is_empty());
    }
}

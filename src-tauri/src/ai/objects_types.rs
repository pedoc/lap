//! Model-independent object observations. Scores are model scores, not calibrated probabilities.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObjectLabel {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LabelSet {
    pub namespace: String,
    pub version: String,
    pub classes: Vec<ObjectLabel>,
}
impl LabelSet {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        if !super::types::safe_id(&self.namespace)
            || !super::types::safe_id(&self.version)
            || self.classes.is_empty()
            || self.classes.len() > 2000
        {
            return Err("Invalid object label-set namespace, version or class count".into());
        }
        for label in &self.classes {
            if !super::types::safe_id(&label.id)
                || !ids.insert(label.id.clone())
                || label.name.trim().is_empty()
                || label.name.len() > 256
                || label.name.chars().any(char::is_control)
            {
                return Err("Object label IDs must be unique and names nonempty".into());
            }
        }
        Ok(())
    }
    pub fn label(&self, index: usize) -> Result<ObjectClass, String> {
        let label = self
            .classes
            .get(index)
            .ok_or("Detector returned an unknown class index")?;
        Ok(ObjectClass {
            key: format!("{}@{}:{}", self.namespace, self.version, label.id),
            source_id: label.id.clone(),
            name: label.name.clone(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectClass {
    pub key: String,
    pub source_id: String,
    pub name: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectObservation {
    pub id: String,
    pub class: ObjectClass,
    pub score: Option<f32>,
    #[serde(default)]
    pub reviewed: bool,
    pub bounds: [f32; 4],
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectDocument {
    pub image_width: u32,
    pub image_height: u32,
    pub objects: Vec<ObjectObservation>,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub class_index: usize,
    pub score: f32,
    pub bounds: [f32; 4],
}
pub fn clip_bounds([x, y, w, h]: [f32; 4]) -> Option<[f32; 4]> {
    if ![x, y, w, h].iter().all(|n| n.is_finite()) || w <= 0. || h <= 0. {
        return None;
    }
    let l = x.max(0.);
    let t = y.max(0.);
    let r = (x + w).min(1.);
    let b = (y + h).min(1.);
    if r <= l || b <= t {
        return None;
    }
    Some([l, t, r - l, b - t])
}
fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let area = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.)
        * ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.);
    let total = a[2] * a[3] + b[2] * b[3] - area;
    if total > 0. { area / total } else { 0. }
}
pub fn finish(
    mut candidates: Vec<Candidate>,
    labels: &LabelSet,
    threshold: f32,
    nms: f32,
    limit: usize,
    size: (u32, u32),
) -> Result<ObjectDocument, String> {
    if size.0 == 0
        || size.1 == 0
        || !threshold.is_finite()
        || !(0.0..=1.0).contains(&threshold)
        || !nms.is_finite()
        || !(0.0..=1.0).contains(&nms)
        || !(1..=300).contains(&limit)
    {
        return Err("Invalid detector postprocessing settings".into());
    }
    if candidates.iter().any(|c| {
        c.class_index >= labels.classes.len()
            || !c.score.is_finite()
            || !(0.0..=1.0).contains(&c.score)
            || !c.bounds.iter().all(|n| n.is_finite())
            || c.bounds[2] < 0.
            || c.bounds[3] < 0.
    }) {
        return Err("Detector returned invalid scores, coordinates or class IDs".into());
    }
    candidates.retain(|c| c.score >= threshold);
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut truncated = candidates.len() > 2000;
    candidates.truncate(2000);
    let mut kept: Vec<Candidate> = Vec::new();
    for mut candidate in candidates {
        let Some(bounds) = clip_bounds(candidate.bounds) else {
            continue;
        };
        candidate.bounds = bounds;
        if nms > 0.
            && kept.iter().any(|old| {
                old.class_index == candidate.class_index && iou(old.bounds, bounds) > nms
            })
        {
            continue;
        }
        if kept.len() >= limit {
            truncated = true;
            break;
        }
        kept.push(candidate);
    }
    let objects = kept
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            Ok(ObjectObservation {
                id: format!("object-{i}"),
                class: labels.label(c.class_index)?,
                score: Some(c.score),
                reviewed: false,
                bounds: c.bounds,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(ObjectDocument {
        image_width: size.0,
        image_height: size.1,
        objects,
        truncated,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn labels() -> LabelSet {
        LabelSet {
            namespace: "test".into(),
            version: "1".into(),
            classes: vec![
                ObjectLabel {
                    id: "dog".into(),
                    name: "dog".into(),
                },
                ObjectLabel {
                    id: "cat".into(),
                    name: "cat".into(),
                },
            ],
        }
    }
    #[test]
    fn classes_are_namespaced_and_unknown_ids_are_rejected() {
        let mut l = labels();
        assert!(l.validate().is_ok());
        assert_eq!(l.label(0).unwrap().key, "test@1:dog");
        assert!(l.label(2).is_err());
        l.classes[1].id = "dog".into();
        assert!(l.validate().is_err());
    }
    #[test]
    fn nms_is_class_aware_and_limits_do_not_claim_completeness() {
        let c = |class, score| Candidate {
            class_index: class,
            score,
            bounds: [0.1, 0.1, 0.3, 0.3],
        };
        let doc = finish(
            vec![c(0, 0.9), c(0, 0.8), c(1, 0.7)],
            &labels(),
            0.3,
            0.5,
            100,
            (100, 80),
        )
        .unwrap();
        assert_eq!(doc.objects.len(), 2);
        assert!(!doc.truncated);
        let doc = finish(vec![c(0, 0.9), c(1, 0.7)], &labels(), 0.3, 0., 1, (100, 80)).unwrap();
        assert!(doc.truncated);
        assert_eq!(doc.objects.len(), 1);
    }
    #[test]
    fn invalid_output_is_a_failure_not_a_false_empty_result() {
        assert!(
            finish(
                vec![Candidate {
                    class_index: 9,
                    score: 0.9,
                    bounds: [0., 0., 1., 1.]
                }],
                &labels(),
                0.3,
                0.5,
                100,
                (100, 100)
            )
            .is_err()
        );
        assert!(
            finish(
                vec![Candidate {
                    class_index: 0,
                    score: f32::NAN,
                    bounds: [0., 0., 1., 1.]
                }],
                &labels(),
                0.3,
                0.5,
                100,
                (100, 100)
            )
            .is_err()
        );
        assert_eq!(clip_bounds([-0.1, 0., 0.3, 0.5]).unwrap()[0], 0.);
        assert!(clip_bounds([1.1, 0., 0.1, 0.5]).is_none());
    }
}

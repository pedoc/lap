//! Object rules never equate unknown/failed/unsupported detections with absence.
use super::{objects_store, settings, types::Task};
use rusqlite::ToSql;
use serde_json::Value;
pub fn condition(
    field: &str,
    operator: &str,
    value: &Value,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    let profile = settings::active(Task::Detection)?.profile();
    condition_on(&profile, field, operator, value)
}
fn condition_on(
    profile: &str,
    field: &str,
    operator: &str,
    value: &Value,
) -> Result<(String, Vec<Box<dyn ToSql>>), String> {
    let (condition, args) = match field {
        "object_state" => {
            let state = value
                .as_str()
                .filter(|s| *s != "all")
                .ok_or("Object state required")?;
            let condition = objects_store::state_condition("?", state)?;
            let condition = match operator {
                "is" => condition,
                "is_not" => format!("NOT ({condition})"),
                _ => return Err("Unsupported object state operator".into()),
            };
            (
                condition,
                vec![Box::new(profile.to_owned()) as Box<dyn ToSql>],
            )
        }
        "object_class" => {
            let key = value.as_str().ok_or("Object class required")?;
            let (op, n) = match operator {
                "has" => ("gte", 1),
                "not_has" => ("eq", 0),
                _ => return Err("Unsupported object class operator".into()),
            };
            objects_store::count_condition(profile, key, op, n)?
        }
        "object_count" => {
            let key = value
                .get("classKey")
                .and_then(Value::as_str)
                .ok_or("Object count class required")?;
            let n = value
                .get("count")
                .and_then(Value::as_i64)
                .ok_or("Object count must be an integer")?;
            if !matches!(operator, "gte" | "lte" | "eq") {
                return Err("Unsupported object count operator".into());
            }
            objects_store::count_condition(profile, key, operator, n)?
        }
        _ => return Err("Unknown object rule".into()),
    };
    Ok((format!("a.file_type IN(1,3) AND ({condition})"), args))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn object_rules_validate_values_and_bind_class_keys() {
        for value in [json!(-1), json!(1.5), json!("2"), json!(10001)] {
            assert!(
                condition_on(
                    "p",
                    "object_count",
                    "eq",
                    &json!({"classKey":"coco2017-80@1:16","count":value})
                )
                .is_err()
            );
        }
        assert!(condition_on("p", "object_state", "is", &json!("all")).is_err());
        assert!(condition_on("p", "object_state", "is", &json!("missing")).is_err());
        assert!(condition_on("p", "object_class", "is", &json!("dog")).is_err());
        let (negative, args) =
            condition_on("p", "object_class", "not_has", &json!("quote'key")).unwrap();
        assert!(!negative.contains("quote'key"));
        assert!(negative.contains("eff.complete=1"));
        assert!(negative.contains("object_classes support"));
        assert_eq!(args.len(), 4);
        let (positive, _) = condition_on(
            "p",
            "object_count",
            "gte",
            &json!({"classKey":"dog","count":2}),
        )
        .unwrap();
        assert!(!positive.contains("eff.complete=1"));
    }
}

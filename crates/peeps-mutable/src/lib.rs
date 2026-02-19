use facet_value::Value;

#[derive(Debug, PartialEq)]
pub enum PatchOp {
    Add { path: String, value: Value },
    Remove { path: String },
    Replace { path: String, value: Value },
}

pub type Patch = Vec<PatchOp>;

pub fn diff(_before: &Value, _after: &Value) -> Patch {
    todo!("implemented after tests are written")
}

pub fn is_empty(patch: &Patch) -> bool {
    patch.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_value::VObject;

    fn object(entries: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
        let mut object = VObject::new();
        for (key, value) in entries {
            object.insert(key, value);
        }
        object.into()
    }

    #[test]
    fn diff_emits_replace_for_changed_scalar_field() {
        let before = object([("waiter_count", 1u64.into()), ("state", "empty".into())]);
        let after = object([("waiter_count", 2u64.into()), ("state", "empty".into())]);

        let patch = diff(&before, &after);

        assert_eq!(
            patch,
            vec![PatchOp::Replace {
                path: "/waiter_count".to_string(),
                value: 2u64.into(),
            }]
        );
    }

    #[test]
    fn diff_emits_remove_when_field_disappears() {
        let before = object([("buffer", 16u64.into()), ("closed", false.into())]);
        let after = object([("closed", false.into())]);

        let patch = diff(&before, &after);

        assert_eq!(
            patch,
            vec![PatchOp::Remove {
                path: "/buffer".to_string(),
            }]
        );
    }

    #[test]
    fn diff_is_empty_when_values_are_identical() {
        let value = object([("x", 1u64.into())]);
        let patch = diff(&value, &value);
        assert!(is_empty(&patch));
    }
}

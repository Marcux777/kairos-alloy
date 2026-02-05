use crate::value_objects::action_type::ActionType;

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub action_type: ActionType,
    pub size: f64,
    pub reason: Option<String>,
}

impl Action {
    pub fn hold() -> Self {
        Self {
            action_type: ActionType::Hold,
            size: 0.0,
            reason: None,
        }
    }
}

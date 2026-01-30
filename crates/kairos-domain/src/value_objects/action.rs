use crate::value_objects::action_type::ActionType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Action {
    pub action_type: ActionType,
    pub size: f64,
}

impl Action {
    pub fn hold() -> Self {
        Self {
            action_type: ActionType::Hold,
            size: 0.0,
        }
    }
}


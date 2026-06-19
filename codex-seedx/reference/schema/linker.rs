use std::collections::HashMap;

pub fn schemas() -> HashMap<String, String> {
    HashMap::from([
        ("shared".to_string(), include_str!("definitions/shared.yml").to_string()),
        (
            "execution".to_string(),
            include_str!("definitions/execution.yml").to_string(),
        ),
        (
            "node_execution".to_string(),
            include_str!("definitions/node_execution.yml").to_string(),
        ),
        (
            "flow/root".to_string(),
            include_str!("definitions/flow/root.yml").to_string(),
        ),
        (
            "flow/connection".to_string(),
            include_str!("definitions/flow/connection.yml").to_string(),
        ),
        (
            "flow/destination".to_string(),
            include_str!("definitions/flow/destination.yml").to_string(),
        ),
        (
            "flow/node".to_string(),
            include_str!("definitions/flow/node.yml").to_string(),
        ),
        (
            "flow/history".to_string(),
            include_str!("definitions/flow/history.yml").to_string(),
        ),
    ])
}

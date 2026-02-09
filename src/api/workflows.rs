/// Workflow operations
use super::error::GitHubError;
use super::http::{GITHUB_API_BASE, ResponseHandler, add_auth_header};
use crate::api::models::{
    Workflow, WorkflowDispatchInput, WorkflowDispatchInputType, WorkflowDispatchInputValue,
    WorkflowsResponse,
};
use base64::Engine;
use reqwest::{Client, StatusCode};
use serde_yaml::Value as YamlValue;
use tracing::info;

/// List workflows for a repository
pub async fn list_workflows(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<Vec<Workflow>, GitHubError> {
    info!("Fetching workflows for {}/{}", owner, repo);

    let request = client.get(format!(
        "{}/repos/{}/{}/actions/workflows",
        GITHUB_API_BASE, owner, repo
    ));

    let request = add_auth_header(request, token);
    let response = request.send().await?;
    let workflows_response: WorkflowsResponse = response_handler.handle_response(response).await?;
    Ok(workflows_response.workflows)
}

#[derive(serde::Deserialize)]
struct WorkflowContentsResponse {
    content: String,
    encoding: Option<String>,
}

/// Fetch workflow_dispatch inputs from a workflow file
pub async fn get_workflow_dispatch_inputs(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
    workflow_path: &str,
    reference: Option<&str>,
) -> Result<Vec<WorkflowDispatchInput>, GitHubError> {
    let mut request = client.get(format!(
        "{}/repos/{}/{}/contents/{}",
        GITHUB_API_BASE, owner, repo, workflow_path
    ));

    if let Some(reference) = reference {
        request = request.query(&[("ref", reference)]);
    }

    let request = add_auth_header(request, token);
    let response = request.send().await?;
    let contents: WorkflowContentsResponse = response_handler.handle_response(response).await?;

    let encoding = contents.encoding.unwrap_or_else(|| "base64".to_string());
    if encoding != "base64" {
        return Err(GitHubError::ApiError(format!(
            "Unsupported workflow encoding: {}",
            encoding
        )));
    }

    let encoded = contents.content.replace('\n', "");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            GitHubError::ApiError(format!("Failed to decode workflow file: {}", error))
        })?;
    let yaml = String::from_utf8(decoded).map_err(|error| {
        GitHubError::ApiError(format!("Workflow file is not valid UTF-8: {}", error))
    })?;

    parse_workflow_dispatch_inputs(&yaml)
}

/// Dispatch a workflow
pub async fn dispatch_workflow(
    client: &Client,
    token: &Option<String>,
    owner: &str,
    repo: &str,
    workflow_id: &str,
    ref_name: &str,
    inputs: Option<serde_json::Value>,
) -> Result<(), GitHubError> {
    info!("Dispatching workflow {} on ref {}", workflow_id, ref_name);

    let mut body = serde_json::json!({
        "ref": ref_name
    });

    if let Some(inputs) = inputs {
        body["inputs"] = inputs;
    }

    let request = client
        .post(format!(
            "{}/repos/{}/{}/actions/workflows/{}/dispatches",
            GITHUB_API_BASE, owner, repo, workflow_id
        ))
        .json(&body);

    let request = add_auth_header(request, token);
    let response = request.send().await?;

    if response.status() == StatusCode::NO_CONTENT {
        info!("Workflow dispatched successfully");
        Ok(())
    } else {
        // For error cases, try to parse as JSON error
        Err(GitHubError::ApiError(format!(
            "Failed to dispatch workflow: {}",
            response.status()
        )))
    }
}

fn parse_workflow_dispatch_inputs(yaml: &str) -> Result<Vec<WorkflowDispatchInput>, GitHubError> {
    let document: YamlValue = serde_yaml::from_str(yaml).map_err(|error| {
        GitHubError::ApiError(format!("Failed to parse workflow YAML: {}", error))
    })?;

    let Some(on_value) = find_yaml_mapping_value(&document, "on") else {
        return Ok(Vec::new());
    };
    let Some(dispatch_value) = find_yaml_mapping_value(on_value, "workflow_dispatch") else {
        return Ok(Vec::new());
    };

    let dispatch_map = match dispatch_value {
        YamlValue::Mapping(map) => map,
        YamlValue::Null => return Ok(Vec::new()),
        _ => return Ok(Vec::new()),
    };

    let Some(inputs_value) = find_yaml_mapping_value_in_map(dispatch_map, "inputs") else {
        return Ok(Vec::new());
    };

    let inputs_map = match inputs_value {
        YamlValue::Mapping(map) => map,
        _ => return Ok(Vec::new()),
    };

    let mut inputs = Vec::new();
    for (key, value) in inputs_map {
        let Some(name) = yaml_value_as_string(key) else {
            continue;
        };
        inputs.push(parse_workflow_dispatch_input(&name, value));
    }

    Ok(inputs)
}

fn parse_workflow_dispatch_input(name: &str, value: &YamlValue) -> WorkflowDispatchInput {
    let mut input = WorkflowDispatchInput {
        name: name.to_string(),
        description: None,
        required: false,
        input_type: WorkflowDispatchInputType::String,
        default_value: None,
        options: Vec::new(),
    };

    let YamlValue::Mapping(map) = value else {
        return input;
    };

    if let Some(description) = find_yaml_string(map, "description") {
        input.description = Some(description);
    }

    if let Some(required) = find_yaml_bool(map, "required") {
        input.required = required;
    }

    if let Some(input_type) = find_yaml_string(map, "type") {
        input.input_type = match input_type.as_str() {
            "choice" => WorkflowDispatchInputType::Choice,
            "boolean" => WorkflowDispatchInputType::Boolean,
            "environment" => WorkflowDispatchInputType::Environment,
            _ => WorkflowDispatchInputType::String,
        };
    }

    if let Some(default_value) = find_yaml_mapping_value_in_map(map, "default") {
        input.default_value = parse_default_value(default_value, &input.input_type);
    }

    if let Some(options_value) = find_yaml_mapping_value_in_map(map, "options")
        && let YamlValue::Sequence(values) = options_value
    {
        input.options = values.iter().filter_map(yaml_value_as_string).collect();
    }

    input
}

fn parse_default_value(
    value: &YamlValue,
    input_type: &WorkflowDispatchInputType,
) -> Option<WorkflowDispatchInputValue> {
    match value {
        YamlValue::Bool(value) => Some(WorkflowDispatchInputValue::Boolean(*value)),
        YamlValue::Number(value) => Some(WorkflowDispatchInputValue::String(value.to_string())),
        YamlValue::String(value) => match input_type {
            WorkflowDispatchInputType::Boolean => value
                .parse::<bool>()
                .ok()
                .map(WorkflowDispatchInputValue::Boolean)
                .or_else(|| Some(WorkflowDispatchInputValue::String(value.clone()))),
            _ => Some(WorkflowDispatchInputValue::String(value.clone())),
        },
        _ => None,
    }
}

fn find_yaml_mapping_value<'a>(value: &'a YamlValue, key: &str) -> Option<&'a YamlValue> {
    let YamlValue::Mapping(map) = value else {
        return None;
    };

    find_yaml_mapping_value_in_map(map, key)
}

fn find_yaml_mapping_value_in_map<'a>(
    map: &'a serde_yaml::Mapping,
    key: &str,
) -> Option<&'a YamlValue> {
    map.iter().find_map(|(map_key, map_value)| match map_key {
        YamlValue::String(map_key) if map_key == key => Some(map_value),
        _ => None,
    })
}

fn find_yaml_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.iter().find_map(|(map_key, map_value)| match map_key {
        YamlValue::String(map_key) if map_key == key => yaml_value_as_string(map_value),
        _ => None,
    })
}

fn find_yaml_bool(map: &serde_yaml::Mapping, key: &str) -> Option<bool> {
    map.iter().find_map(|(map_key, map_value)| match map_key {
        YamlValue::String(map_key) if map_key == key => match map_value {
            YamlValue::Bool(value) => Some(*value),
            _ => None,
        },
        _ => None,
    })
}

fn yaml_value_as_string(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::String(value) => Some(value.clone()),
        YamlValue::Number(value) => Some(value.to_string()),
        YamlValue::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_publish_workflow_inputs() {
        let yaml = include_str!("../../.github/workflows/publish.yml");
        let inputs = parse_workflow_dispatch_inputs(yaml).expect("parse workflow inputs");

        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[0].name, "tag");
        assert!(inputs[0].required);
        assert_eq!(inputs[0].input_type, WorkflowDispatchInputType::String);
        assert_eq!(inputs[1].name, "name");
        assert!(!inputs[1].required);
        assert_eq!(inputs[2].name, "notes");
    }

    #[test]
    fn parse_typed_workflow_inputs() {
        let yaml = r#"
on:
  workflow_dispatch:
    inputs:
      environment:
        description: Target environment
        type: choice
        options:
          - staging
          - production
        default: production
      dry_run:
        type: boolean
        default: true
      target:
        type: environment
        required: true
"#;

        let inputs = parse_workflow_dispatch_inputs(yaml).expect("parse workflow inputs");
        let environment = inputs
            .iter()
            .find(|input| input.name == "environment")
            .unwrap();
        assert_eq!(environment.input_type, WorkflowDispatchInputType::Choice);
        assert_eq!(environment.options, vec!["staging", "production"]);
        assert_eq!(
            environment.default_value,
            Some(WorkflowDispatchInputValue::String("production".to_string()))
        );

        let dry_run = inputs.iter().find(|input| input.name == "dry_run").unwrap();
        assert_eq!(dry_run.input_type, WorkflowDispatchInputType::Boolean);
        assert_eq!(
            dry_run.default_value,
            Some(WorkflowDispatchInputValue::Boolean(true))
        );

        let target = inputs.iter().find(|input| input.name == "target").unwrap();
        assert_eq!(target.input_type, WorkflowDispatchInputType::Environment);
        assert!(target.required);
    }
}

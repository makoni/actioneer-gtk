/// Workflow operations
use super::error::GitHubError;
use super::http::{GITHUB_API_BASE, ResponseHandler, add_auth_header};
use crate::api::models::{
    Workflow, WorkflowDispatchInput, WorkflowDispatchInputType, WorkflowDispatchInputValue,
    WorkflowRun, WorkflowsResponse,
};
use base64::Engine;
use chrono::Utc;
use reqwest::{Client, StatusCode};
use serde_yaml::Value as YamlValue;
use tracing::{info, warn};

/// List workflows for a repository
pub async fn list_workflows(
    client: &Client,
    token: &Option<String>,
    response_handler: &ResponseHandler,
    owner: &str,
    repo: &str,
) -> Result<Vec<Workflow>, GitHubError> {
    info!("Fetching workflows for {}/{}", owner, repo);
    let cache_key = format!("workflows:{owner}/{repo}");

    let request = client.get(format!(
        "{}/repos/{}/{}/actions/workflows",
        GITHUB_API_BASE, owner, repo
    ));

    let request = add_auth_header(request, token);
    let request = response_handler.apply_cache_headers(request, Some(cache_key.as_str()));
    let response = request.send().await?;
    let workflows_response: WorkflowsResponse = response_handler
        .handle_response(response, Some(cache_key.as_str()))
        .await?;
    Ok(workflows_response.workflows)
}

#[derive(Debug, serde::Deserialize)]
struct WorkflowContentsResponse {
    content: String,
    encoding: Option<String>,
}

#[derive(serde::Deserialize)]
struct WorkflowDispatchResponse {
    workflow_run_id: i64,
    html_url: Option<String>,
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
    let cache_key = format!(
        "workflow-contents:{owner}/{repo}:{workflow_path}:ref={}",
        reference.unwrap_or_default()
    );
    let mut request = client.get(format!(
        "{}/repos/{}/{}/contents/{}",
        GITHUB_API_BASE, owner, repo, workflow_path
    ));

    if let Some(reference) = reference {
        request = request.query(&[("ref", reference)]);
    }

    let request = add_auth_header(request, token);
    let request = response_handler.apply_cache_headers(request, Some(cache_key.as_str()));
    let response = request.send().await?;
    let contents_payload: serde_json::Value = response_handler
        .handle_response(response, Some(cache_key.as_str()))
        .await?;
    let contents = parse_workflow_contents_response(contents_payload, workflow_path)?;

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

fn parse_workflow_contents_response(
    value: serde_json::Value,
    workflow_path: &str,
) -> Result<WorkflowContentsResponse, GitHubError> {
    match value {
        serde_json::Value::Array(_) => Err(GitHubError::ApiError(format!(
            "Workflow path '{}' resolved to a directory listing instead of a workflow file",
            workflow_path
        ))),
        serde_json::Value::Object(map) => {
            let entry_type = map.get("type").and_then(serde_json::Value::as_str);
            if matches!(
                entry_type,
                Some("dir") | Some("submodule") | Some("symlink")
            ) || !map.contains_key("content")
            {
                return Err(GitHubError::ApiError(format!(
                    "Workflow path '{}' did not resolve to a regular file response",
                    workflow_path
                )));
            }

            serde_json::from_value(serde_json::Value::Object(map)).map_err(|error| {
                GitHubError::ApiError(format!(
                    "Failed to parse workflow file response for '{}': {}",
                    workflow_path, error
                ))
            })
        }
        other => Err(GitHubError::ApiError(format!(
            "Workflow path '{}' returned an unexpected contents payload: {}",
            workflow_path, other
        ))),
    }
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
) -> Result<Option<WorkflowRun>, GitHubError> {
    info!("Dispatching workflow {} on ref {}", workflow_id, ref_name);

    let mut body = serde_json::json!({
        "ref": ref_name,
        "return_run_details": true
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
    let status = response.status();

    if status == StatusCode::NO_CONTENT || status == StatusCode::OK {
        info!("Workflow dispatched successfully");
        let body = response.bytes().await?;
        parse_dispatch_run_details(status, body.as_ref(), workflow_id, ref_name)
    } else {
        // For error cases, try to parse as JSON error
        Err(GitHubError::ApiError(format!(
            "Failed to dispatch workflow: {}",
            status
        )))
    }
}

fn parse_dispatch_run_details(
    status: StatusCode,
    body: &[u8],
    workflow_id: &str,
    ref_name: &str,
) -> Result<Option<WorkflowRun>, GitHubError> {
    if status != StatusCode::OK && status != StatusCode::NO_CONTENT {
        return Err(GitHubError::ApiError(format!(
            "Unexpected dispatch response status: {}",
            status
        )));
    }

    if body.is_empty() {
        return Ok(None);
    }

    let details: WorkflowDispatchResponse = match serde_json::from_slice(body) {
        Ok(details) => details,
        Err(error) => {
            warn!("Failed to parse dispatch response details: {}", error);
            return Ok(None);
        }
    };

    let now = Utc::now().to_rfc3339();
    Ok(Some(WorkflowRun {
        id: details.workflow_run_id,
        run_number: None,
        workflow_id: workflow_id.parse::<i64>().ok(),
        name: None,
        display_title: None,
        head_branch: Some(ref_name.to_string()),
        head_commit: None,
        status: Some("queued".to_string()),
        conclusion: None,
        run_started_at: Some(now.clone()),
        event: Some("workflow_dispatch".to_string()),
        created_at: Some(now.clone()),
        updated_at: Some(now),
        html_url: details.html_url,
        actor: None,
        triggering_actor: None,
    }))
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
    use crate::api::models::WorkflowRunsResponse;

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

    #[test]
    fn parse_dispatch_response_returns_provisional_run() {
        let body = br#"{
            "workflow_run_id": 4242,
            "run_url": "https://api.github.com/repos/octo/repo/actions/runs/4242",
            "html_url": "https://github.com/octo/repo/actions/runs/4242"
        }"#;

        let run = parse_dispatch_run_details(StatusCode::OK, body, "21001", "main")
            .expect("parse dispatch response")
            .expect("provisional run");

        assert_eq!(run.id, 4242);
        assert_eq!(run.workflow_id, Some(21001));
        assert_eq!(run.status.as_deref(), Some("queued"));
        assert_eq!(run.head_branch.as_deref(), Some("main"));
        assert_eq!(
            run.html_url.as_deref(),
            Some("https://github.com/octo/repo/actions/runs/4242")
        );
    }

    #[test]
    fn parse_dispatch_response_accepts_legacy_empty_success_body() {
        let run = parse_dispatch_run_details(StatusCode::NO_CONTENT, b"", "21001", "main")
            .expect("legacy success should still pass");
        assert!(run.is_none());
    }

    #[test]
    fn parse_workflow_contents_response_accepts_file_payload() {
        let payload = serde_json::json!({
            "type": "file",
            "encoding": "base64",
            "content": "bmFtZTogQ0kK"
        });

        let parsed =
            parse_workflow_contents_response(payload, ".github/workflows/ci.yml").expect("file");

        assert_eq!(parsed.encoding.as_deref(), Some("base64"));
        assert_eq!(parsed.content, "bmFtZTogQ0kK");
    }

    #[test]
    fn parse_workflow_contents_response_rejects_directory_listing() {
        let payload = serde_json::json!([
            { "type": "file", "path": ".github/workflows/ci.yml" }
        ]);

        let error = parse_workflow_contents_response(payload, ".github/workflows")
            .expect_err("directory listing should fail");

        assert!(
            error
                .to_string()
                .contains("resolved to a directory listing instead of a workflow file")
        );
    }

    #[test]
    fn workflow_runs_response_deserializes_with_trimmed_schema() {
        let json = serde_json::json!({
            "total_count": 1,
            "workflow_runs": [
                {
                    "id": 42,
                    "run_number": 7,
                    "workflow_id": 9,
                    "status": "queued",
                    "conclusion": null
                }
            ]
        });

        let response: WorkflowRunsResponse =
            serde_json::from_value(json).expect("workflow runs response");
        assert_eq!(response.total_count, 1);
        assert_eq!(response.workflow_runs[0].id, 42);
        assert_eq!(response.workflow_runs[0].workflow_id, Some(9));
    }
}

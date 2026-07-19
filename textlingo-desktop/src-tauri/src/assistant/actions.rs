use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::storage::load_config;

use super::{
    dto::{
        AssistantActionAuditRequest, AssistantActionExecution, AssistantCommandError,
        AssistantTaskTimelineEvent,
    },
    service::AssistantService,
};

const ALLOWED_READ_ONLY_ACTIONS: &[&str] = &[
    "get_current_material",
    "list_materials",
    "open_material",
    "open_source",
];

const EXTERNAL_OR_WRITE_MARKERS: &[&str] = &[
    "anki", "zotero", "mineru", "sync", "import", "write", "delete", "update", "create", "export",
    "external",
];

#[derive(Debug, Clone)]
pub struct EvaluatedAssistantAction {
    pub audit: AssistantActionAuditRequest,
    pub navigation: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantNavigationDispatch {
    pub event_name: &'static str,
    pub payload: Value,
}

pub fn assistant_action_from_result_payload(payload: &Value) -> Option<Value> {
    payload
        .get("action")
        .filter(|action| !action.is_null())
        .cloned()
}

pub fn evaluate_assistant_action(action: &Value) -> EvaluatedAssistantAction {
    let Some(object) = action.as_object() else {
        return rejected(
            "unknown",
            "assistant_action_invalid",
            "Assistant action must be a JSON object",
            action.clone(),
        );
    };
    let action_kind = object
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    if !ALLOWED_READ_ONLY_ACTIONS.contains(&action_kind) {
        let lower = action_kind.to_ascii_lowercase();
        let code = if EXTERNAL_OR_WRITE_MARKERS
            .iter()
            .any(|marker| lower.contains(marker))
        {
            "assistant_external_or_write_action_forbidden"
        } else {
            "assistant_action_not_registered"
        };
        return rejected(
            action_kind,
            code,
            "Only registered read-only navigation actions are allowed in phase one",
            action.clone(),
        );
    }

    let navigation = match action_kind {
        "get_current_material" | "list_materials" => None,
        "open_material" | "open_source" => {
            let Some(material_id) = object
                .get("material_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return rejected(
                    action_kind,
                    "assistant_action_invalid",
                    "material_id is required for this action",
                    action.clone(),
                );
            };
            let mut navigation = json!({
                "kind": action_kind,
                "material_id": material_id,
            });
            if let Some(locator) = object.get("locator") {
                navigation["locator"] = locator.clone();
            }
            Some(navigation)
        }
        _ => None,
    };

    EvaluatedAssistantAction {
        audit: AssistantActionAuditRequest {
            action_kind: action_kind.to_string(),
            status: "executed".to_string(),
            code: None,
            message: Some("Read-only Assistant action accepted".to_string()),
            payload: action.clone(),
        },
        navigation,
    }
}

pub fn navigation_dispatch_after_audit(
    evaluated: &EvaluatedAssistantAction,
    audit_event: &AssistantTaskTimelineEvent,
) -> Option<AssistantNavigationDispatch> {
    if evaluated.audit.status != "executed" || audit_event.event_type != "action_executed" {
        return None;
    }
    let navigation = evaluated.navigation.as_ref()?;
    let kind = navigation.get("kind").and_then(Value::as_str)?;
    let material_id = navigation.get("material_id").and_then(Value::as_str)?;
    match kind {
        "open_material" => Some(AssistantNavigationDispatch {
            event_name: "agent://open-material",
            payload: json!({ "materialId": material_id }),
        }),
        "open_source" => {
            let mut payload = json!({ "materialId": material_id });
            if let Some(locator) = navigation.get("locator") {
                payload["locator"] = locator.clone();
            }
            Some(AssistantNavigationDispatch {
                event_name: "agent://open-source",
                payload,
            })
        }
        _ => None,
    }
}

pub async fn execute_registered_assistant_action(
    app_handle: &AppHandle,
    task_id: &str,
    action: &Value,
) -> Result<AssistantActionExecution, AssistantCommandError> {
    let evaluated = evaluate_assistant_action(action);
    let config = load_config(app_handle)
        .map_err(|error| AssistantCommandError::new("assistant_config_error", error))?
        .unwrap_or_default();
    let audit_event = AssistantService::from_config(&config)?
        .audit_action(task_id, &evaluated.audit)
        .await?;
    let dispatch = navigation_dispatch_after_audit(&evaluated, &audit_event);
    if let Some(dispatch) = dispatch.as_ref() {
        app_handle
            .emit(dispatch.event_name, &dispatch.payload)
            .map_err(|error| {
                AssistantCommandError::new("assistant_navigation_emit_failed", error.to_string())
            })?;
    }
    let navigation = if dispatch.is_some() {
        evaluated.navigation.clone()
    } else {
        None
    };
    let status = audit_event
        .metadata
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or(if audit_event.event_type == "action_executed" {
            "executed"
        } else {
            "rejected"
        })
        .to_string();
    let code = audit_event
        .metadata
        .get("code")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or(evaluated.audit.code);
    let message = audit_event.message.clone().or(evaluated.audit.message);
    Ok(AssistantActionExecution {
        task_id: task_id.to_string(),
        action_kind: evaluated.audit.action_kind,
        status,
        code,
        message,
        navigation,
        audit_event,
    })
}

fn rejected(
    action_kind: &str,
    code: &str,
    message: &str,
    payload: Value,
) -> EvaluatedAssistantAction {
    EvaluatedAssistantAction {
        audit: AssistantActionAuditRequest {
            action_kind: action_kind.to_string(),
            status: "rejected".to_string(),
            code: Some(code.to_string()),
            message: Some(message.to_string()),
            payload,
        },
        navigation: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_accepts_read_only_navigation_and_rejects_external_writes() {
        let allowed = evaluate_assistant_action(&json!({
            "kind": "open_source",
            "material_id": "material-1",
            "locator": {"kind": "segment", "segment_id": "segment-1"}
        }));
        assert_eq!(allowed.audit.status, "executed");
        assert_eq!(allowed.navigation.unwrap()["material_id"], "material-1");

        for kind in [
            "anki.sync",
            "zotero.write",
            "mineru.parse",
            "delete_material",
        ] {
            let rejected = evaluate_assistant_action(&json!({"kind": kind}));
            assert_eq!(rejected.audit.status, "rejected");
            assert_eq!(
                rejected.audit.code.as_deref(),
                Some("assistant_external_or_write_action_forbidden")
            );
            assert!(rejected.navigation.is_none());
        }
    }

    fn audit(event_type: &str) -> AssistantTaskTimelineEvent {
        AssistantTaskTimelineEvent {
            id: "event-1".to_string(),
            task_id: "task-1".to_string(),
            event_type: event_type.to_string(),
            from_status: None,
            to_status: None,
            stage: None,
            message: None,
            error: None,
            metadata: json!({}),
            created_at: "2026-07-15T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn rejected_and_unknown_actions_never_create_navigation_dispatches() {
        for action in [
            json!({"kind": "anki_write"}),
            json!({"kind": "unknown_action"}),
        ] {
            let evaluated = evaluate_assistant_action(&action);
            assert!(
                navigation_dispatch_after_audit(&evaluated, &audit("action_rejected")).is_none()
            );
            assert!(
                navigation_dispatch_after_audit(&evaluated, &audit("action_executed")).is_none()
            );
        }
    }

    #[test]
    fn open_material_navigation_requires_backend_execution_audit() {
        let evaluated = evaluate_assistant_action(&json!({
            "kind": "open_material",
            "material_id": "material-1"
        }));
        assert!(navigation_dispatch_after_audit(&evaluated, &audit("action_rejected")).is_none());

        let dispatch = navigation_dispatch_after_audit(&evaluated, &audit("action_executed"))
            .expect("executed audit should authorize navigation");
        assert_eq!(dispatch.event_name, "agent://open-material");
        assert_eq!(dispatch.payload, json!({ "materialId": "material-1" }));
    }

    #[test]
    fn open_source_uses_compatible_camel_case_event_payload() {
        let evaluated = evaluate_assistant_action(&json!({
            "kind": "open_source",
            "material_id": "material-1",
            "locator": {"version": 1, "kind": "segment", "segment_order": 0, "total_segments": 1}
        }));
        let dispatch = navigation_dispatch_after_audit(&evaluated, &audit("action_executed"))
            .expect("executed audit should authorize source navigation");
        assert_eq!(dispatch.event_name, "agent://open-source");
        assert_eq!(dispatch.payload["materialId"], "material-1");
        assert_eq!(dispatch.payload["locator"]["kind"], "segment");
    }

    #[test]
    fn informational_read_actions_are_audited_without_navigation() {
        for kind in ["get_current_material", "list_materials"] {
            let evaluated = evaluate_assistant_action(&json!({"kind": kind}));
            assert_eq!(evaluated.audit.status, "executed");
            assert!(evaluated.navigation.is_none());
            assert!(
                navigation_dispatch_after_audit(&evaluated, &audit("action_executed")).is_none()
            );
        }
    }
}

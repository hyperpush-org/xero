use serde_json::{json, Value as JsonValue};
use xero_desktop_lib::commands::{
    validate_code_history_operation_contract,
    validate_code_history_operation_status_request_contract,
    validate_code_history_operation_status_response_contract,
    validate_selective_undo_request_contract, validate_selective_undo_response_contract,
    validate_session_rollback_request_contract, validate_session_rollback_response_contract,
    CodeHistoryOperationDto, CodeHistoryOperationStatusRequestDto,
    CodeHistoryOperationStatusResponseDto, SelectiveUndoRequestDto, SelectiveUndoResponseDto,
    SessionRollbackRequestDto, SessionRollbackResponseDto,
};

#[test]
fn code_history_contracts_accept_selective_undo_and_session_rollback_shapes() {
    let undo_request: SelectiveUndoRequestDto = serde_json::from_value(json!({
        "projectId": "project-1",
        "operationId": "history-op-1",
        "target": {
            "targetKind": "hunks",
            "targetId": "code-change-1:src/app.ts:hunk-1",
            "changeGroupId": "code-change-1",
            "filePath": "src/app.ts",
            "hunkIds": ["hunk-1"]
        },
        "expectedWorkspaceEpoch": 11
    }))
    .expect("selective undo request");
    validate_selective_undo_request_contract(&undo_request).expect("valid undo request");

    let undo_response: SelectiveUndoResponseDto = serde_json::from_value(json!({
        "operation": sample_operation()
    }))
    .expect("selective undo response");
    validate_selective_undo_response_contract(&undo_response).expect("valid undo response");

    let rollback_request: SessionRollbackRequestDto = serde_json::from_value(json!({
        "projectId": "project-1",
        "operationId": "history-op-2",
        "target": {
            "targetKind": "run_boundary",
            "targetId": "run-1:boundary-1",
            "agentSessionId": "agent-session-1",
            "runId": "run-1",
            "boundaryId": "boundary-1",
            "changeGroupId": "code-change-1"
        },
        "expectedWorkspaceEpoch": 12
    }))
    .expect("session rollback request");
    validate_session_rollback_request_contract(&rollback_request).expect("valid rollback request");

    let mut rollback_operation = sample_operation();
    rollback_operation["operationId"] = json!("history-op-2");
    rollback_operation["mode"] = json!("session_rollback");
    rollback_operation["target"] = json!({
        "targetKind": "run_boundary",
        "targetId": "run-1:boundary-1"
    });
    rollback_operation["conflicts"][0]["operationId"] = json!("history-op-2");
    rollback_operation["conflicts"][0]["targetId"] = json!("run-1:boundary-1");

    let rollback_response: SessionRollbackResponseDto = serde_json::from_value(json!({
        "operation": rollback_operation
    }))
    .expect("session rollback response");
    validate_session_rollback_response_contract(&rollback_response)
        .expect("valid rollback response");

    let status_request: CodeHistoryOperationStatusRequestDto = serde_json::from_value(json!({
        "projectId": "project-1",
        "operationId": "history-op-1"
    }))
    .expect("operation status request");
    validate_code_history_operation_status_request_contract(&status_request)
        .expect("valid status request");

    let status_response: CodeHistoryOperationStatusResponseDto = serde_json::from_value(json!({
        "operation": sample_operation()
    }))
    .expect("operation status response");
    validate_code_history_operation_status_response_contract(&status_response)
        .expect("valid status response");
}

#[test]
fn code_history_operation_contract_rejects_unknown_modes_and_statuses() {
    let mut unknown_mode = sample_operation();
    unknown_mode["mode"] = json!("snapshot_restore");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(unknown_mode).is_err());

    let mut unknown_status = sample_operation();
    unknown_status["status"] = json!("restored");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(unknown_status).is_err());
}

#[test]
fn code_history_operation_contract_requires_ids_paths_and_conflicts() {
    let mut missing_operation_id = sample_operation();
    missing_operation_id
        .as_object_mut()
        .expect("operation object")
        .remove("operationId");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(missing_operation_id).is_err());

    let mut blank_operation_id: CodeHistoryOperationDto =
        serde_json::from_value(sample_operation()).expect("operation");
    blank_operation_id.operation_id.clear();
    assert!(validate_code_history_operation_contract(&blank_operation_id).is_err());

    let mut missing_target_id = sample_operation();
    missing_target_id["target"]
        .as_object_mut()
        .expect("target object")
        .remove("targetId");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(missing_target_id).is_err());

    let mut missing_affected_paths = sample_operation();
    missing_affected_paths
        .as_object_mut()
        .expect("operation object")
        .remove("affectedPaths");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(missing_affected_paths).is_err());

    let mut empty_affected_paths: CodeHistoryOperationDto =
        serde_json::from_value(sample_operation()).expect("operation");
    empty_affected_paths.affected_paths.clear();
    assert!(validate_code_history_operation_contract(&empty_affected_paths).is_err());

    let mut missing_conflict_payload: CodeHistoryOperationDto =
        serde_json::from_value(sample_operation()).expect("operation");
    missing_conflict_payload.conflicts.clear();
    assert!(validate_code_history_operation_contract(&missing_conflict_payload).is_err());

    let mut malformed_conflict = sample_operation();
    malformed_conflict["conflicts"][0]
        .as_object_mut()
        .expect("conflict object")
        .remove("path");
    assert!(serde_json::from_value::<CodeHistoryOperationDto>(malformed_conflict).is_err());
}

fn sample_operation() -> JsonValue {
    json!({
        "projectId": "project-1",
        "operationId": "history-op-1",
        "mode": "selective_undo",
        "status": "conflicted",
        "target": {
            "targetKind": "file_change",
            "targetId": "code-change-1:src/app.ts"
        },
        "affectedPaths": ["src/app.ts"],
        "conflicts": [
            {
                "operationId": "history-op-1",
                "targetId": "code-change-1:src/app.ts",
                "path": "src/app.ts",
                "kind": "text_overlap",
                "message": "Current content changed lines selected for undo.",
                "baseHash": "sha256:base",
                "selectedHash": "sha256:selected",
                "currentHash": "sha256:current",
                "hunkIds": ["hunk-1"]
            }
        ],
        "workspaceHead": {
            "projectId": "project-1",
            "headId": "code-head-1",
            "treeId": "code-tree-1",
            "workspaceEpoch": 12,
            "latestHistoryOperationId": "history-op-1",
            "updatedAt": "2026-05-06T12:00:01Z"
        },
        "patchAvailability": {
            "projectId": "project-1",
            "targetChangeGroupId": "code-change-1",
            "available": true,
            "affectedPaths": ["src/app.ts"],
            "fileChangeCount": 1,
            "textHunkCount": 1,
            "unavailableReason": null
        },
        "resultCommitId": null,
        "resultChangeGroupId": null,
        "createdAt": "2026-05-06T12:00:00Z",
        "updatedAt": "2026-05-06T12:00:01Z"
    })
}

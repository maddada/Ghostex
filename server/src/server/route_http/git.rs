//! Git routes: typed Git/GitHub/worktree operations, project Git state, the ship workflow, pull requests, commit messages, and the board's start-work routes.

use super::*;

pub(super) async fn route_git_http(
    request: RouteHttpRequest,
) -> Result<RoutedResponse, RouteHttpRequest> {
    let RouteHttpRequest {
        state,
        endpoint,
        request_id,
        body_json,
        token_extension_id,
        caller,
    } = request;
    Ok(match endpoint.path.as_str() {
        "/api/runGitAction"
        | "/api/runGitHubAction"
        | "/api/runWorktreeAction"
        | "/api/runBeadsAction" => {
            handle_typed_operation_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/startBoardWork" => {
            handle_board_start_work_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/associateBoardSession" => {
            handle_board_associate_session_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/generateCommitMessage" => {
            handle_generate_commit_message_http(&state, endpoint.path, request_id, &body_json).await
        }
        "/api/readProjectGitState" => {
            crate::project_git_state::handle_read_project_git_state_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/runGitShipWorkflow" => {
            crate::git_ship_workflow::handle_run_git_ship_workflow_http(
                &state,
                endpoint.path,
                request_id,
                &body_json,
            )
            .await
        }
        "/api/createPullRequest" => {
            handle_create_pull_request_http(&state, endpoint.path, request_id, &body_json).await
        }
        _ => {
            return Err(RouteHttpRequest {
                state,
                endpoint,
                request_id,
                body_json,
                token_extension_id,
                caller,
            })
        }
    })
}

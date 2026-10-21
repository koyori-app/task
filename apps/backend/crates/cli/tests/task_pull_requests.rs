//! タスクと PR の連携（`tasks pr`）。仕様は `docs/features/tasks/9.github-tasks.md` §8。

mod common;

use common::*;
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

const PR_87: &str = "87878787-8787-4787-8787-878787878787";
const PR_88: &str = "88888888-8888-4888-8888-888888888888";

async fn mount_project_lookup(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/tenants/{TENANT}/projects")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([project_json()])))
        .mount(&harness.server)
        .await;
}

fn pull_request_json(id: &str, number: i32) -> serde_json::Value {
    json!({
        "id": id,
        "project_id": PROJECT_ID,
        "host": "github",
        "host_url": "https://github.com",
        "repo_owner": "acme",
        "repo_name": "app",
        "number": number,
        "title": "直す",
        "author_handle": "yupix",
        "state": "open",
        "html_url": format!("https://github.com/acme/app/pull/{number}"),
        "merged_at": null,
        "merge_effects_processed_at": null,
        "commits_complete": true,
        "sources": ["manual"],
        "closes": false,
        "unresolved": null,
    })
}

async fn mount_forge(harness: &Harness) {
    Mock::given(method("GET"))
        .and(path(project_path("tasks/APP-7/forge")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "pull_requests": [pull_request_json(PR_87, 87), pull_request_json(PR_88, 88)],
            "commits": [],
            "branches": [],
            "repository": {
                "host": "github",
                "host_url": "https://github.com",
                "repo_owner": "acme",
                "repo_name": "app",
                "html_url": "https://github.com/acme/app",
            },
        })))
        .mount(&harness.server)
        .await;
}

#[tokio::test]
async fn list_reads_the_tasks_forge_links() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_forge(&harness).await;

    for argv in [
        vec!["task", "tasks", "pr", "list", "APP-7"],
        vec!["task", "tasks", "pr", "list", "APP-7", "--json"],
    ] {
        assert_eq!(harness.run(&argv).await.unwrap(), 0, "{argv:?}");
    }
}

#[tokio::test]
async fn link_posts_the_number_and_whether_a_merge_completes_the_task() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks/APP-7/forge/pull-requests")))
        .and(body_json(json!({ "number": 87, "closes": true })))
        .respond_with(ResponseTemplate::new(201).set_body_json(pull_request_json(PR_87, 87)))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "tasks", "pr", "link", "APP-7", "87", "--closes"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn link_explains_that_the_project_is_not_connected_to_github() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    Mock::given(method("POST"))
        .and(path(project_path("tasks/APP-7/forge/pull-requests")))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({ "message": "github-integration-missing" })),
        )
        .expect(1)
        .mount(&harness.server)
        .await;

    let err = harness
        .run(&["task", "tasks", "pr", "link", "APP-7", "87"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 1);
    assert!(
        err.message.contains("not connected to a GitHub repository"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn unlink_deletes_the_link_by_the_id_of_the_numbered_pull_request() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_forge(&harness).await;
    Mock::given(method("DELETE"))
        .and(path(project_path(&format!(
            "tasks/APP-7/forge/pull-requests/{PR_88}"
        ))))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&harness.server)
        .await;

    let code = harness
        .run(&["task", "tasks", "pr", "unlink", "APP-7", "88"])
        .await
        .unwrap();
    assert_eq!(code, 0);
}

#[tokio::test]
async fn unlink_of_a_pull_request_that_is_not_linked_is_not_found() {
    let harness = harness().await;
    mount_project_lookup(&harness).await;
    mount_forge(&harness).await;

    let err = harness
        .run(&["task", "tasks", "pr", "unlink", "APP-7", "99"])
        .await
        .unwrap_err();
    assert_eq!(err.exit_code, 5);
    assert!(
        err.message.contains("#99 is not linked to APP-7"),
        "{}",
        err.message
    );
    let requests = harness.server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| request.method.as_str() != "DELETE"),
        "{requests:?}"
    );
}

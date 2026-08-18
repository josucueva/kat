//! Integration tests for KAT v0.4 Porcelain Commands and Machine Interface Envelope.

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn kat_bin() -> String {
    env!("CARGO_BIN_EXE_kat").to_string()
}

fn setup_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = Command::new(kat_bin())
        .arg("init")
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    dir
}

#[test]
fn v04_porcelain_author_context_check_commit_flow() {
    let dir = setup_repo();

    // 1. Author declarative claims via JSON array
    let claims_file = dir.path().join("claims.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Auth Spec",
    "description": "System shall verify JWT",
    "handle": "@req-auth"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/implementation",
    "title": "Auth Service",
    "description": "Auth module",
    "handle": "@imp-auth"
  },
  {
    "kind": "link_element",
    "source_ref": "@imp-auth",
    "relationship_type_id": "kat.core/realizes",
    "target_ref": "@req-auth"
  }
]"#,
    )
    .unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["interface_schema_version"], 1);
    assert_eq!(json["data"]["claims_processed"], 3);
    assert_eq!(json["data"]["operations_staged"], 3);

    let req_id = json["data"]["workflow_references"]["@req-auth"]
        .as_str()
        .unwrap()
        .to_string();

    // 2. Check draft transaction status
    let output = Command::new(kat_bin())
        .args(["status", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);

    // 3. Run porcelain check (clean repository with advisory findings exit code 0)
    let output = Command::new(kat_bin())
        .args(["check", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["repository_clean"], true);

    // 4. Porcelain commit
    let output = Command::new(kat_bin())
        .args(["commit", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["operations_count"], 3);

    // 5. Retrieve porcelain context over committed accepted state
    let output = Command::new(kat_bin())
        .args([
            "context",
            &req_id,
            "--direction",
            "both",
            "--categorize",
            "--json",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
}

#[test]
fn v04_porcelain_abort_clears_staged_claims() {
    let dir = setup_repo();

    let claims_file = dir.path().join("claims.json");
    fs::write(
        &claims_file,
        r#"[{"kind": "create_element", "type_id": "kat.core/requirement", "title": "Temp Req"}]"#,
    )
    .unwrap();

    let status = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap()])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());

    // Abort
    let output = Command::new(kat_bin())
        .args(["abort", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["aborted"], true);
}

#[test]
fn v04_porcelain_author_fail_closed_and_example_test() {
    let dir = setup_repo();

    // 1. kat author --example
    let output = Command::new(kat_bin())
        .args(["author", "--example", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(json.is_array());
    assert_eq!(json[0]["kind"], "create_element");

    // 2. Malformed claims JSON -> fail closed with AuthorParseError
    let claims_file = dir.path().join("bad_claims.json");
    fs::write(&claims_file, "this is definitely not valid json").unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "AuthorParseError");

    // 3. Verify 0 draft transaction operations staged
    let output = Command::new(kat_bin())
        .args(["status", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["data"]["knowledge"]["total_elements"], 0);
}

#[test]
fn v04_porcelain_ontology_guidance_test() {
    let dir = setup_repo();

    let claims_file = dir.path().join("invalid_rel.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "kind": "create_element",
    "type_id": "kat.core/artifact",
    "title": "Auth Spec File",
    "handle": "@art-auth"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/validation",
    "title": "Auth Test Suite",
    "handle": "@val-auth"
  },
  {
    "kind": "link_element",
    "relationship_type_id": "kat.core/represents",
    "source_ref": "@art-auth",
    "target_ref": "@val-auth"
  }
]"#,
    )
    .unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "AuthorCompilationFailed");

    let err_msg = json["error"]["message"].as_str().unwrap();
    assert!(err_msg.contains("requires source in"));
    assert!(err_msg.contains("kat.core/implementation"));
}

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
    "CreateElement": {
      "type_id": "kat.core/requirement",
      "title": "Auth Spec",
      "description": "System shall verify JWT",
      "handle": "@req-auth"
    }
  },
  {
    "CreateElement": {
      "type_id": "kat.core/implementation",
      "title": "Auth Service",
      "description": "Auth module",
      "handle": "@imp-auth"
    }
  },
  {
    "LinkElement": {
      "source_ref": "@imp-auth",
      "relationship_type_id": "kat.core/realizes",
      "target_ref": "@req-auth"
    }
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

    let claims_file = dir.path().join("claims.txt");
    fs::write(&claims_file, "create kat.core/requirement \"Temp Req\"\n").unwrap();

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

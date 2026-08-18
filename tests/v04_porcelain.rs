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
fn v04_porcelain_unknown_claim_kind_fails_closed() {
    let dir = setup_repo();

    let claims_file = dir.path().join("bad_kind.json");
    fs::write(
        &claims_file,
        r#"[{"kind": "unknown_claim_type", "type_id": "kat.core/requirement", "title": "Bad"}]"#,
    )
    .unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "AUTHOR_PARSE_ERROR");
    assert!(json["error"]["details"]["reason"].is_string());

    // Verify 0 session operations staged
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
fn v04_porcelain_claim_n_malformed_rejects_whole_batch() {
    let dir = setup_repo();

    let claims_file = dir.path().join("partially_bad.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Good Claim 1",
    "handle": "@req-1"
  },
  {
    "kind": "create_element",
    "title": "Missing required type_id field"
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
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "AUTHOR_PARSE_ERROR");

    // Verify atomic rejection: 0 claims staged
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
fn v04_porcelain_empty_or_whitespace_input_succeeds() {
    let dir = setup_repo();

    let claims_file = dir.path().join("empty.json");
    fs::write(&claims_file, "   \n\t  ").unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["claims_processed"], 0);
    assert_eq!(json["data"]["operations_staged"], 0);

    // Verify no draft session created
    let session_file = dir.path().join(".kat/work/change/session.json");
    assert!(!session_file.exists());
}

#[test]
fn v04_porcelain_duplicate_workflow_handle_fails_atomically() {
    let dir = setup_repo();

    let claims_file = dir.path().join("dup_handles.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Req 1",
    "handle": "@dup-handle"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Req 2",
    "handle": "@dup-handle"
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
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "AUTHOR_COMPILATION_FAILED");
    assert!(json["error"]["message"].as_str().unwrap().contains("duplicate workflow reference handle"));
}

#[test]
fn v04_porcelain_example_outside_kat_repository() {
    let temp_dir = tempfile::tempdir().unwrap();

    let output = Command::new(kat_bin())
        .args(["author", "--example"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("create_element"));
    assert!(stdout.contains("kat.core/requirement"));
}

#[test]
fn v04_porcelain_example_template_is_valid_and_stageable() {
    let dir = setup_repo();

    // 1. Get template output from --example
    let output = Command::new(kat_bin())
        .args(["author", "--example"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();

    // 2. Write example template to file and author it
    let claims_file = dir.path().join("example_claims.json");
    fs::write(&claims_file, &stdout).unwrap();

    let output = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap(), "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["claims_processed"], 3);
    assert_eq!(json["data"]["operations_staged"], 3);
}

#[test]
fn v04_porcelain_example_conflicts() {
    let dir = setup_repo();

    // 1. --example with CLAIMS_FILE -> exit 2
    let output = Command::new(kat_bin())
        .args(["author", "--example", "some_file.json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));

    // 2. --example with --json -> exit 2
    let output = Command::new(kat_bin())
        .args(["author", "--example", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn v04_porcelain_legacy_v040_externally_tagged_json() {
    let dir = setup_repo();

    let claims_file = dir.path().join("legacy_claims.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "CreateElement": {
      "type_id": "kat.core/requirement",
      "title": "Legacy Req",
      "handle": "@legacy-req"
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
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["data"]["claims_processed"], 1);
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
    assert_eq!(json["error"]["code"], "AUTHOR_COMPILATION_FAILED");

    let err_msg = json["error"]["message"].as_str().unwrap();
    assert!(err_msg.contains("requires source in"));
    assert!(err_msg.contains("kat.core/implementation"));
}

#[test]
fn v04_graph_quality_gq03_gq04_frozen_semantics_test() {
    let dir = setup_repo();

    let claims_file = dir.path().join("gq_claims.json");
    fs::write(
        &claims_file,
        r#"[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Auth Spec Requirement",
    "handle": "@req"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/implementation",
    "title": "Auth Engine Implementation",
    "handle": "@imp"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/design-decision",
    "title": "Token Strategy Decision",
    "handle": "@dec"
  },
  {
    "kind": "link_element",
    "relationship_type_id": "kat.core/realizes",
    "source_ref": "@imp",
    "target_ref": "@req"
  }
]"#,
    )
    .unwrap();

    let output1 = Command::new(kat_bin())
        .args(["author", claims_file.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output1.status.success());

    let output2 = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output2.status.success());

    // Run kat check
    let output_chk = Command::new(kat_bin())
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output_chk.status.success());
    let chk_out = String::from_utf8(output_chk.stdout).unwrap();

    // Verify GQ-03 message: Implementation has no modeled Artifact representation route
    assert!(chk_out.contains("GQ-03"));
    assert!(chk_out.contains("has no modeled Artifact representation route"));

    // Verify GQ-04 message: Design Decision has no consequence route through kat.core/addresses or kat.core/guides
    assert!(chk_out.contains("GQ-04"));
    assert!(chk_out.contains("has no consequence route through kat.core/addresses or kat.core/guides"));

    let out1_str = String::from_utf8(output1.stdout).unwrap();
    let imp_id = out1_str
        .lines()
        .find(|l| l.contains("@imp"))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let req_id = out1_str
        .lines()
        .find(|l| l.contains("@req"))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let dec_id = out1_str
        .lines()
        .find(|l| l.contains("@dec"))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();

    // Now resolve GQ-03 and GQ-04 by adding Artifact represents Implementation & Design Decision addresses Requirement
    // Cross-Change references use stable UUIDs (handles expired on commit)
    let fix_claims_content = format!(
        r#"[
  {{
    "kind": "create_element",
    "type_id": "kat.core/artifact",
    "title": "Auth Service File",
    "handle": "@art"
  }},
  {{
    "kind": "link_element",
    "relationship_type_id": "kat.core/represents",
    "source_ref": "@art",
    "target_ref": "{imp_id}"
  }},
  {{
    "kind": "link_element",
    "relationship_type_id": "kat.core/addresses",
    "source_ref": "{dec_id}",
    "target_ref": "{req_id}"
  }}
]"#
    );

    let fix_claims = dir.path().join("gq_fix_claims.json");
    fs::write(&fix_claims, fix_claims_content).unwrap();

    let output3 = Command::new(kat_bin())
        .args(["author", fix_claims.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output3.status.success());

    let output4 = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output4.status.success());

    // Verify GQ-03 and GQ-04 are resolved
    let output_chk2 = Command::new(kat_bin())
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output_chk2.status.success());
    let chk2_out = String::from_utf8(output_chk2.stdout).unwrap();
    assert!(!chk2_out.contains("GQ-03"));
    assert!(!chk2_out.contains("GQ-04"));
}

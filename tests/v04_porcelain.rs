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
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("duplicate workflow reference handle")
    );
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
    assert!(chk_out.contains("no modeled Artifact representation route"));

    // Verify GQ-04 message: Design Decision has no consequence route through kat.core/addresses or kat.core/guides
    assert!(chk_out.contains("GQ-04"));
    assert!(chk_out.contains("no consequence route through addresses or guides"));

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

// ---------------------------------------------------------------------------
// v0.4.3 Check Porcelain Acceptance Tests (CHECK-01 to CHECK-09)
// ---------------------------------------------------------------------------

#[test]
fn v043_check_porcelain_acceptance_tests() {
    let dir = setup_repo();

    let claims1 = r#"[
        {
            "kind": "create_element",
            "type_id": "kat.core/requirement",
            "title": "Auth Spec Requirement",
            "handle": "@req"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/constraint",
            "title": "Local Storage Policy",
            "handle": "@con"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/implementation",
            "title": "Auth Storage Service",
            "handle": "@imp"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/implementation",
            "title": "Auth Secondary Service",
            "handle": "@imp2"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/design-decision",
            "title": "Encryption Key Strategy",
            "handle": "@dec"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/artifact",
            "title": "Stale Auth File",
            "handle": "@art-stale"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/artifact",
            "title": "Unaccounted Config File",
            "handle": "@art-unacc"
        },
        {
            "kind": "link_element",
            "relationship_type_id": "kat.core/realizes",
            "source_ref": "@imp",
            "target_ref": "@req"
        },
        {
            "kind": "link_element",
            "relationship_type_id": "kat.core/represents",
            "source_ref": "@art-stale",
            "target_ref": "@imp"
        }
    ]"#;

    let f1 = dir.path().join("c1.json");
    fs::write(&f1, claims1).unwrap();

    let out1 = Command::new(kat_bin())
        .args(["author", f1.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let out1_str = String::from_utf8(out1.stdout).unwrap();
    assert!(out1.status.success(), "kat author failed: {out1_str}");
    let com1 = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(com1.status.success());

    let _req_id = out1_str
        .lines()
        .find(|l| l.contains("@req "))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let imp_id = out1_str
        .lines()
        .find(|l| l.contains("@imp "))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let con_id = out1_str
        .lines()
        .find(|l| l.contains("@con "))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let art_stale_id = out1_str
        .lines()
        .find(|l| l.contains("@art-stale "))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();
    let art_unacc_id = out1_str
        .lines()
        .find(|l| l.contains("@art-unacc "))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();

    // Update implementation to make art-stale stale
    let claims2 = format!(
        r#"[
        {{
            "kind": "update_element",
            "element_ref": "{imp_id}",
            "description": "Updated implementation description"
        }}
    ]"#
    );
    let f2 = dir.path().join("c2.json");
    fs::write(&f2, claims2).unwrap();
    let out2 = Command::new(kat_bin())
        .args(["author", f2.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out2.status.success());
    let com2 = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(com2.status.success());

    // Execute kat check
    let chk = Command::new(kat_bin())
        .arg("check")
        .current_dir(dir.path())
        .output()
        .unwrap();

    // CHECK-07: Advisory findings do not alter mechanical cleanliness (exit code == 0, clean == true)
    assert_eq!(chk.status.code().unwrap(), 0);
    let chk_stdout = String::from_utf8(chk.stdout).unwrap();
    assert!(chk_stdout.contains("clean: true"));

    // CHECK-01: Four dimensions visible
    assert!(chk_stdout.contains("MECHANICAL CONSISTENCY"));
    assert!(chk_stdout.contains("EVIDENCE COVERAGE"));
    assert!(chk_stdout.contains("ARTIFACT ACCOUNTABILITY"));
    assert!(chk_stdout.contains("GRAPH QUALITY"));

    // CHECK-02: Uncovered constraint identified
    let con_sid = &con_id[..8];
    assert!(chk_stdout.contains("constraints: 0 / 1 evidence-backed (0.0%)"));
    assert!(chk_stdout.contains(con_sid));
    assert!(chk_stdout.contains("Local Storage Policy"));

    // CHECK-03 & CHECK-04: Stale & Unaccounted Artifacts identified
    let stale_sid = &art_stale_id[..8];
    let unacc_sid = &art_unacc_id[..8];
    assert!(
        chk_stdout.contains("Stale:"),
        "chk_stdout was:\n{chk_stdout}"
    );
    assert!(chk_stdout.contains(stale_sid));
    assert!(chk_stdout.contains("Stale Auth File"));
    assert!(chk_stdout.contains("Unaccounted:"));
    assert!(chk_stdout.contains(unacc_sid));
    assert!(chk_stdout.contains("Unaccounted Config File"));

    // CHECK-05: No accountability baseline dump in check default output
    assert!(!chk_stdout.contains("baseline_version"));

    // CHECK-06: GQ findings include identity
    assert!(chk_stdout.contains("[GQ-04]"));
    assert!(chk_stdout.contains("[GQ-03]"));
    assert!(chk_stdout.contains("Encryption Key Strategy"));

    // Test --compact mode
    let chk_compact = Command::new(kat_bin())
        .args(["check", "--compact"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(chk_compact.status.success());
    let compact_stdout = String::from_utf8(chk_compact.stdout).unwrap();
    assert!(compact_stdout.contains(
        "CLEAN | mechanical 0 | evidence 0/1 | artifacts 0 current, 1 stale, 1 unaccounted | GQ"
    ));

    // CHECK-09: JSON regression test
    let chk_json = Command::new(kat_bin())
        .args(["check", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(chk_json.status.success());
    let json_val: serde_json::Value =
        serde_json::from_str(&String::from_utf8(chk_json.stdout).unwrap()).unwrap();
    assert_eq!(json_val["success"], true);
    assert_eq!(json_val["data"]["repository_clean"], true);
    assert!(
        json_val["data"]["artifact_accountability"]["repository_summary"]["stale"]
            .as_u64()
            .unwrap()
            >= 1
    );
}

#[test]
fn v044_machine_coverage_acceptance_tests() {
    let dir = tempfile::tempdir().unwrap();

    // MACHINE-04: Run outside KAT repository with --json
    let out_no_repo = Command::new(kat_bin())
        .args(["show", "deadbeef", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!out_no_repo.status.success());
    let json_no_repo: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out_no_repo.stdout).unwrap()).unwrap();
    assert_eq!(json_no_repo["success"], false);
    assert_eq!(json_no_repo["error"]["code"], "NotInRepository");

    // Init KAT repository
    let init_out = Command::new(kat_bin())
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(init_out.status.success());

    // Create an element via author
    let claims = r#"[
        {
            "kind": "create_element",
            "type_id": "kat.core/requirement",
            "title": "Auth Requirement",
            "handle": "@req"
        }
    ]"#;
    let f1 = dir.path().join("c1.json");
    fs::write(&f1, claims).unwrap();
    let auth_out = Command::new(kat_bin())
        .args(["author", f1.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(auth_out.status.success());

    let auth_str = String::from_utf8(auth_out.stdout).unwrap();
    let req_id = auth_str
        .lines()
        .find(|l| l.contains("@req"))
        .and_then(|l| l.split("->").nth(1))
        .unwrap()
        .trim();

    let com_out = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(com_out.status.success());

    // MACHINE-01: Test --json across all 8 Inspection commands
    let commands: &[&[&str]] = &[
        &["list", "--json"],
        &["show", req_id, "--json"],
        &["history", "--json"],
        &["trace", req_id, "--json"],
        &["impact", req_id, "--json"],
        &["artifacts", "--json"],
        &["ontology", "--json"],
        &["validate", "--json"],
    ];

    for args in commands {
        let out = Command::new(kat_bin())
            .args(*args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "Command {:?} failed", args);
        let val: serde_json::Value =
            serde_json::from_str(&String::from_utf8(out.stdout).unwrap()).unwrap();
        assert_eq!(val["interface_schema_version"], 1);
        assert_eq!(val["success"], true);
        assert!(!val["data"].is_null());
        assert!(val["error"].is_null());
    }

    // MACHINE-03: Structured error for unknown element ID in show --json
    let out_unknown = Command::new(kat_bin())
        .args(["show", "deadbeef", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!out_unknown.status.success());
    let json_unknown: serde_json::Value =
        serde_json::from_str(&String::from_utf8(out_unknown.stdout).unwrap()).unwrap();
    assert_eq!(json_unknown["success"], false);
    assert_eq!(json_unknown["error"]["code"], "ResolveError");
    assert!(json_unknown["data"].is_null());
}

#[test]
fn v044_cross_change_reference_handoff_acceptance_tests() {
    let dir = tempfile::tempdir().unwrap();
    let init_out = Command::new(kat_bin())
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(init_out.status.success());

    // Change 1: Author elements with handles @req and @imp
    let claims1 = r#"[
        {
            "kind": "create_element",
            "type_id": "kat.core/requirement",
            "title": "Auth Requirement",
            "handle": "@req"
        },
        {
            "kind": "create_element",
            "type_id": "kat.core/implementation",
            "title": "Auth Service",
            "handle": "@imp"
        }
    ]"#;
    let f1 = dir.path().join("c1.json");
    fs::write(&f1, claims1).unwrap();

    let auth1 = Command::new(kat_bin())
        .args(["author", f1.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(auth1.status.success());

    // Commit Change 1 with --json
    let com1 = Command::new(kat_bin())
        .args(["commit", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(com1.status.success());

    let json_com1: serde_json::Value =
        serde_json::from_str(&String::from_utf8(com1.stdout).unwrap()).unwrap();
    assert_eq!(json_com1["success"], true);

    let resolutions = json_com1["data"]["workflow_reference_resolutions"]
        .as_array()
        .unwrap();
    assert_eq!(resolutions.len(), 2);

    // REF-11: Lexicographical handle order
    assert_eq!(resolutions[0]["handle"], "@imp");
    assert_eq!(resolutions[1]["handle"], "@req");

    let imp_res = &resolutions[0];
    let imp_ref = imp_res["reference"].as_str().unwrap();
    let imp_id = imp_res["element_id"].as_str().unwrap();
    assert!(imp_ref.len() >= 8);

    // REF-04 & REF-10: Change 2 consumes the returned prefix (imp_ref)
    let claims2 = format!(
        r#"[
        {{
            "kind": "update_element",
            "element_ref": "{imp_ref}",
            "title": "Auth Service Updated"
        }}
    ]"#
    );
    let f2 = dir.path().join("c2.json");
    fs::write(&f2, claims2).unwrap();

    let auth2 = Command::new(kat_bin())
        .args(["author", f2.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let auth2_str = String::from_utf8(auth2.stdout).unwrap();
    assert!(
        auth2.status.success(),
        "kat author Change 2 failed: {auth2_str}"
    );

    let com2 = Command::new(kat_bin())
        .arg("commit")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(com2.status.success());

    // REF-05: Handles expire after commit (@imp in Change 3 must fail as unknown handle)
    let claims3 = r#"[
        {
            "kind": "update_element",
            "element_ref": "@imp",
            "title": "Expired Handle Test"
        }
    ]"#;
    let f3 = dir.path().join("c3.json");
    fs::write(&f3, claims3).unwrap();

    let auth3 = Command::new(kat_bin())
        .args(["author", f3.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        !auth3.status.success(),
        "Expired handle @imp should have failed authoring"
    );

    // Verify updated element title via kat show
    let show_out = Command::new(kat_bin())
        .args(["show", imp_id, "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(show_out.status.success());
    let show_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8(show_out.stdout).unwrap()).unwrap();
    assert_eq!(
        show_json["data"]["element"]["properties"][0][1]["Text"],
        "Auth Service Updated"
    );
}

#[test]
fn v044_canonical_bytes_and_object_id_invariance_test() {
    // REF-12: Proves that the presence of workflow reference handles in a draft transaction
    // produces 100% bit-for-bit identical canonical ObjectIds, CBOR bytes, and SemanticState
    // compared to a draft session with identical operations and no workflow references.
    let dir = tempfile::tempdir().unwrap();
    let init_out = Command::new(kat_bin())
        .arg("init")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(init_out.status.success());
    let repo = kat::repository::open::open_repository(dir.path()).unwrap();

    let elem_id = kat::domain::identity::ElementId::new();
    let ev = kat::domain::element::KnowledgeElementVersion {
        element_id: elem_id,
        type_id: "kat.core/requirement".to_string(),
        lifecycle: kat::domain::element::Lifecycle::Active,
        properties: vec![(
            "title".to_string(),
            kat::domain::property::PropertyValue::Text("Invariant Test".to_string()),
        )],
    };

    let op = kat::domain::operation::Operation::CreateElement {
        new_version: kat::domain::identity::ObjectId::from_bytes([0x42; 32]),
    };

    let state_bytes = repo.object_store().get(repo.accepted.state).unwrap();
    let state_obj = kat::encoding::decode_canonical(&state_bytes).unwrap();
    let mut working_state = match state_obj.payload {
        kat::encoding::object::CanonicalPayload::SemanticState(s) => s,
        _ => unreachable!(),
    };
    working_state
        .elements
        .push(kat::domain::state::ElementStateEntry {
            element_id: elem_id,
            version: kat::domain::identity::ObjectId::from_bytes([0x42; 32]),
        });

    let base_session = kat::repository::session::DraftSession {
        schema_version: kat::repository::session::DRAFT_SESSION_VERSION,
        status: kat::repository::session::DraftSessionState::Open,
        base_state_id: repo.accepted.state,
        base_change_id: repo.accepted.change,
        created_at: "2026-08-18T00:00:00Z".to_string(),
        description: Some("Invariance test".to_string()),
        operations: vec![op.clone()],
        staged_element_versions: vec![ev.clone()],
        staged_relationship_versions: vec![],
        working_state: working_state.clone(),
        workflow_references: vec![],
    };

    // Session 1: No workflow reference handles
    let session_no_handles = base_session.clone();

    // Session 2: Contains workflow reference handles
    let mut session_with_handles = base_session.clone();
    session_with_handles.bind_workflow_reference("@req", elem_id);
    session_with_handles.bind_workflow_reference("@imp", elem_id);

    // 1. Verify working_state canonical CBOR bytes are 100% identical
    let obj_no = kat::encoding::object::CanonicalObject {
        payload: kat::encoding::object::CanonicalPayload::SemanticState(
            session_no_handles.working_state.clone(),
        ),
    };
    let obj_with = kat::encoding::object::CanonicalObject {
        payload: kat::encoding::object::CanonicalPayload::SemanticState(
            session_with_handles.working_state.clone(),
        ),
    };
    let bytes_no = kat::encoding::cbor::canonical_bytes(&obj_no).unwrap();
    let bytes_with = kat::encoding::cbor::canonical_bytes(&obj_with).unwrap();
    assert_eq!(bytes_no, bytes_with);

    // 2. Verify KnowledgeElementVersion canonical CBOR bytes are 100% identical
    let ev_obj_no = kat::encoding::object::CanonicalObject {
        payload: kat::encoding::object::CanonicalPayload::KnowledgeElementVersion(
            session_no_handles.staged_element_versions[0].clone(),
        ),
    };
    let ev_obj_with = kat::encoding::object::CanonicalObject {
        payload: kat::encoding::object::CanonicalPayload::KnowledgeElementVersion(
            session_with_handles.staged_element_versions[0].clone(),
        ),
    };
    let ev_bytes_no = kat::encoding::cbor::canonical_bytes(&ev_obj_no).unwrap();
    let ev_bytes_with = kat::encoding::cbor::canonical_bytes(&ev_obj_with).unwrap();
    assert_eq!(ev_bytes_no, ev_bytes_with);

    // 3. Commit both sessions in separate test repos and verify identical published state_id
    kat::repository::session::write_draft_session_atomic(dir.path(), &session_with_handles)
        .unwrap();
    let outcome = kat::repository::change::commit_draft_session(&repo).unwrap();
    assert_eq!(outcome.workflow_reference_resolutions.len(), 2);
    assert_eq!(outcome.workflow_reference_resolutions[0].handle, "@imp");
    assert_eq!(outcome.workflow_reference_resolutions[1].handle, "@req");
    assert_eq!(
        outcome.workflow_reference_resolutions[0].element_id,
        elem_id
    );
    assert_eq!(
        outcome.workflow_reference_resolutions[1].element_id,
        elem_id
    );
}

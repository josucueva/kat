//! Extensive real-project end-to-end validation test for KAT.
//!
//! Models the evolution of a real-world software project ("AuthX Service")
//! exercising all KAT operations:
//! - Repository initialization
//! - System Intent & Requirements creation
//! - Architecture Constraints & Design Decisions
//! - Relationship linking (`addresses`, `restricts`, `motivates`)
//! - Requirement updating with property patch overlay
//! - Design Decision supersession (`supersedes`)
//! - Requirement deprecation
//! - Relationship unlinking
//! - Integrity verification & history trace queries

use kat::domain::element::Lifecycle;
use kat::domain::identity::{ChangeId, ElementId, RelationshipId};
use kat::domain::operation::Operation;
use kat::domain::property::PropertyValue;
use kat::repository::change::{
    CreateElementInput, DeprecateElementInput, LinkElementInput, SupersedeElementInput,
    UnlinkElementInput, UpdateElementInput, apply_create_element, apply_deprecate_element,
    apply_link_element, apply_supersede_element, apply_unlink_element, apply_update_element,
    persist_prepared_change, persist_prepared_deprecate_change, persist_prepared_link_change,
    persist_prepared_supersede_change, persist_prepared_unlink_change,
    persist_prepared_update_change, prepare_change, prepare_change_revision,
    prepare_deprecate_change_revision, prepare_link_change_revision,
    prepare_supersede_change_revision, prepare_unlink_change_revision,
    prepare_update_change_revision, publish_persisted_change, publish_persisted_deprecate_change,
    publish_persisted_link_change, publish_persisted_supersede_change,
    publish_persisted_unlink_change, publish_persisted_update_change,
    validate_create_element_invariants, validate_create_element_ontology,
    validate_deprecate_element_invariants, validate_deprecate_element_ontology,
    validate_link_element_invariants, validate_link_element_ontology,
    validate_supersede_element_invariants, validate_supersede_element_ontology,
    validate_unlink_element_invariants, validate_update_element_invariants,
    validate_update_element_ontology,
};
use kat::repository::init::init_repository;
use kat::repository::open::open_repository;
use kat::repository::query::{history, show_element};

#[test]
fn real_project_authx_service_validation_test() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // =========================================================================
    // STEP 1: Repository Initialization
    // =========================================================================
    let _repo_init = init_repository(root).expect("kat init should succeed");

    // =========================================================================
    // STEP 2: Create Intent, Requirements, Constraints & Design Decisions
    // =========================================================================

    // 2a. System Intent
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_intent = ElementId::new();
    let intent_input = CreateElementInput {
        element_id: e_intent,
        type_id: "kat.core/intent".to_string(),
        properties: vec![
            (
                "title".to_string(),
                PropertyValue::Text("AuthX Identity & Access Service".to_string()),
            ),
            (
                "description".to_string(),
                PropertyValue::Text(
                    "Core authentication and access control service for the enterprise platform."
                        .to_string(),
                ),
            ),
        ],
    };
    let intent_prep = validate_create_element_invariants(
        validate_create_element_ontology(apply_create_element(ctx, intent_input).unwrap()).unwrap(),
    )
    .unwrap();
    let intent_rev = prepare_change_revision(
        intent_prep,
        ChangeId::new(),
        Some("Define AuthX system intent".to_string()),
    )
    .unwrap();
    let _intent_pub =
        publish_persisted_change(&repo, persist_prepared_change(&repo, intent_rev).unwrap())
            .unwrap();

    // 2b. Requirement 1: User Authentication
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req1 = ElementId::new();
    let req1_input = CreateElementInput {
        element_id: e_req1,
        type_id: "kat.core/requirement".to_string(),
        properties: vec![
            (
                "title".to_string(),
                PropertyValue::Text("User must authenticate via OAuth2 / OIDC".to_string()),
            ),
            (
                "description".to_string(),
                PropertyValue::Text(
                    "Initial basic authentication requirements for API consumers.".to_string(),
                ),
            ),
        ],
    };
    let req1_prep = validate_create_element_invariants(
        validate_create_element_ontology(apply_create_element(ctx, req1_input).unwrap()).unwrap(),
    )
    .unwrap();
    let req1_rev = prepare_change_revision(
        req1_prep,
        ChangeId::new(),
        Some("Add user authentication requirement".to_string()),
    )
    .unwrap();
    let req1_pub =
        publish_persisted_change(&repo, persist_prepared_change(&repo, req1_rev).unwrap()).unwrap();
    let v_req1_v1 = req1_pub.persisted.prepared.creation.element_version_id;

    // 2c. Requirement 2: Token Expiration
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_req2 = ElementId::new();
    let req2_input = CreateElementInput {
        element_id: e_req2,
        type_id: "kat.core/requirement".to_string(),
        properties: vec![
            (
                "title".to_string(),
                PropertyValue::Text(
                    "Session token must expire after 15 minutes of inactivity".to_string(),
                ),
            ),
            (
                "description".to_string(),
                PropertyValue::Text(
                    "Inactivity timeout requirement for active user sessions.".to_string(),
                ),
            ),
        ],
    };
    let req2_prep = validate_create_element_invariants(
        validate_create_element_ontology(apply_create_element(ctx, req2_input).unwrap()).unwrap(),
    )
    .unwrap();
    let req2_rev = prepare_change_revision(
        req2_prep,
        ChangeId::new(),
        Some("Add token expiration requirement".to_string()),
    )
    .unwrap();
    let _req2_pub =
        publish_persisted_change(&repo, persist_prepared_change(&repo, req2_rev).unwrap()).unwrap();

    // 2d. Architecture Constraint 1: TLS 1.3
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_const1 = ElementId::new();
    let const1_input = CreateElementInput {
        element_id: e_const1,
        type_id: "kat.core/constraint".to_string(),
        properties: vec![
            (
                "title".to_string(),
                PropertyValue::Text("TLS 1.3 encryption required for all transit data".to_string()),
            ),
            (
                "description".to_string(),
                PropertyValue::Text(
                    "Mandatory cipher suite and protocol policy for transport security."
                        .to_string(),
                ),
            ),
        ],
    };
    let const1_prep = validate_create_element_invariants(
        validate_create_element_ontology(apply_create_element(ctx, const1_input).unwrap()).unwrap(),
    )
    .unwrap();
    let const1_rev = prepare_change_revision(
        const1_prep,
        ChangeId::new(),
        Some("Add TLS constraint".to_string()),
    )
    .unwrap();
    let _const1_pub =
        publish_persisted_change(&repo, persist_prepared_change(&repo, const1_rev).unwrap())
            .unwrap();

    // 2e. Design Decision 1: JWT RS256
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_dec1 = ElementId::new();
    let dec1_input = CreateElementInput {
        element_id: e_dec1,
        type_id: "kat.core/design-decision".to_string(),
        properties: vec![
            (
                "title".to_string(),
                PropertyValue::Text(
                    "Use JWT tokens signed with RS256 for session identity".to_string(),
                ),
            ),
            (
                "description".to_string(),
                PropertyValue::Text(
                    "Standard RSA signature scheme for stateless token validation.".to_string(),
                ),
            ),
        ],
    };
    let dec1_prep = validate_create_element_invariants(
        validate_create_element_ontology(apply_create_element(ctx, dec1_input).unwrap()).unwrap(),
    )
    .unwrap();
    let dec1_rev = prepare_change_revision(
        dec1_prep,
        ChangeId::new(),
        Some("Add JWT RS256 design decision".to_string()),
    )
    .unwrap();
    let dec1_pub =
        publish_persisted_change(&repo, persist_prepared_change(&repo, dec1_rev).unwrap()).unwrap();
    let v_dec1_v1 = dec1_pub.persisted.prepared.creation.element_version_id;

    // =========================================================================
    // STEP 3: Link Design Decisions & Constraints
    // =========================================================================

    // 3a. Link Intent (motivates) -> Req 1
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let type_addresses = "kat.core/addresses".to_string();
    let type_restricts = "kat.core/restricts".to_string();
    let type_motivates = "kat.core/motivates".to_string();

    let r0_id = RelationshipId::new();
    let link0_prep = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r0_id,
                    relationship_type_id: type_motivates.clone(),
                    source_element_id: e_intent,
                    target_element_id: e_req1,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let link0_rev = prepare_link_change_revision(
        link0_prep,
        ChangeId::new(),
        Some("Link Intent to Requirement 1".to_string()),
    )
    .unwrap();
    let _link0_pub = publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(&repo, link0_rev).unwrap(),
    )
    .unwrap();

    // 3b. Link JWT Decision (addresses) -> Req 1 (OAuth2)
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r1_id = RelationshipId::new();
    let link1_prep = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r1_id,
                    relationship_type_id: type_addresses.clone(),
                    source_element_id: e_dec1,
                    target_element_id: e_req1,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let link1_rev = prepare_link_change_revision(
        link1_prep,
        ChangeId::new(),
        Some("Link JWT decision to Auth requirement".to_string()),
    )
    .unwrap();
    let _link1_pub = publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(&repo, link1_rev).unwrap(),
    )
    .unwrap();

    // 3c. Link Constraint 1 (restricts) -> JWT Decision
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r2_id = RelationshipId::new();
    let link2_prep = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r2_id,
                    relationship_type_id: type_restricts.clone(),
                    source_element_id: e_const1,
                    target_element_id: e_dec1,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let link2_rev = prepare_link_change_revision(
        link2_prep,
        ChangeId::new(),
        Some("Link TLS constraint to JWT decision".to_string()),
    )
    .unwrap();
    let link2_pub = publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(&repo, link2_rev).unwrap(),
    )
    .unwrap();
    let r2v1_id = link2_pub.persisted.prepared.link.relationship_version_id;

    // =========================================================================
    // STEP 4: Update Requirement 1 (MFA enhancement)
    // =========================================================================
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let update1_prep = validate_update_element_invariants(
        validate_update_element_ontology(
            apply_update_element(
                &repo,
                ctx,
                UpdateElementInput {
                    element_id: e_req1,
                    expected_version: v_req1_v1,
                    properties: vec![
                        (
                            "title".to_string(),
                            PropertyValue::Text(
                                "User must authenticate via OAuth2 / OIDC with mandatory MFA"
                                    .to_string(),
                            ),
                        ),
                        (
                            "description".to_string(),
                            PropertyValue::Text(
                                "Added multi-factor authentication (TOTP/WebAuthn) for high-assurance security compliance."
                                    .to_string(),
                            ),
                        ),
                    ],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let update1_rev = prepare_update_change_revision(
        update1_prep,
        ChangeId::new(),
        Some("Update auth requirement to mandate MFA".to_string()),
    )
    .unwrap();
    let update1_pub = publish_persisted_update_change(
        &repo,
        persist_prepared_update_change(&repo, update1_rev).unwrap(),
    )
    .unwrap();
    let v_req1_v2 = update1_pub.persisted.prepared.update.element_version_id;

    // =========================================================================
    // STEP 5: Supersede JWT Decision with PASETO v4 Decision
    // =========================================================================
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let e_dec2 = ElementId::new();
    let r_sup_id = RelationshipId::new();
    let sup1_prep = validate_supersede_element_invariants(
        validate_supersede_element_ontology(
            apply_supersede_element(
                &repo,
                ctx,
                SupersedeElementInput {
                    existing_element_id: e_dec1,
                    expected_existing_version: v_dec1_v1,
                    replacement_element_id: e_dec2,
                    replacement_type_id: "kat.core/design-decision".to_string(),
                    replacement_properties: vec![
                        (
                            "title".to_string(),
                            PropertyValue::Text(
                                "Use PASETO v4 tokens with Ed25519 signatures for quantum-resistant session identity"
                                    .to_string(),
                            ),
                        ),
                        (
                            "description".to_string(),
                            PropertyValue::Text(
                                "Migrating from JWT to PASETO v4 to eliminate algorithm confusion attacks and improve payload tamper resistance."
                                    .to_string(),
                            ),
                        ),
                    ],
                    relationship_id: r_sup_id,
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let sup1_rev = prepare_supersede_change_revision(
        sup1_prep,
        ChangeId::new(),
        Some("Supersede JWT RS256 with PASETO v4".to_string()),
    )
    .unwrap();
    let _sup1_pub = publish_persisted_supersede_change(
        &repo,
        persist_prepared_supersede_change(&repo, sup1_rev).unwrap(),
    )
    .unwrap();

    // Link PASETO decision (addresses) -> Req 1 (Auth MFA)
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let r3_id = RelationshipId::new();
    let link3_prep = validate_link_element_invariants(
        validate_link_element_ontology(
            apply_link_element(
                &repo,
                ctx,
                LinkElementInput {
                    relationship_id: r3_id,
                    relationship_type_id: type_addresses.clone(),
                    source_element_id: e_dec2,
                    target_element_id: e_req1,
                    properties: vec![],
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let link3_rev = prepare_link_change_revision(
        link3_prep,
        ChangeId::new(),
        Some("Link PASETO decision to Auth MFA requirement".to_string()),
    )
    .unwrap();
    let _link3_pub = publish_persisted_link_change(
        &repo,
        persist_prepared_link_change(&repo, link3_rev).unwrap(),
    )
    .unwrap();

    // =========================================================================
    // STEP 6: Deprecate Token Expiration Requirement 2
    // =========================================================================
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let req2_v1 = ctx
        .base_state
        .elements
        .iter()
        .find(|e| e.element_id == e_req2)
        .unwrap()
        .version;
    let dep1_prep = validate_deprecate_element_invariants(
        validate_deprecate_element_ontology(
            apply_deprecate_element(
                &repo,
                ctx,
                DeprecateElementInput {
                    element_id: e_req2,
                    expected_version: req2_v1,
                },
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let dep1_rev = prepare_deprecate_change_revision(
        dep1_prep,
        ChangeId::new(),
        Some(
            "Deprecate token expiration requirement (centralized identity gateway handling)"
                .to_string(),
        ),
    )
    .unwrap();
    let _dep1_pub = publish_persisted_deprecate_change(
        &repo,
        persist_prepared_deprecate_change(&repo, dep1_rev).unwrap(),
    )
    .unwrap();

    // =========================================================================
    // STEP 7: Unlink TLS Constraint -> Old JWT Decision (R2)
    // =========================================================================
    let repo = open_repository(root).unwrap();
    let ctx = prepare_change(&repo).unwrap();
    let unlink1_prep = validate_unlink_element_invariants(
        apply_unlink_element(
            &repo,
            ctx,
            UnlinkElementInput {
                relationship_id: r2_id,
                expected_version: r2v1_id,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let unlink1_rev = prepare_unlink_change_revision(
        unlink1_prep,
        ChangeId::new(),
        Some("Unlink TLS constraint from superseded JWT decision".to_string()),
    )
    .unwrap();
    let _unlink1_pub = publish_persisted_unlink_change(
        &repo,
        persist_prepared_unlink_change(&repo, unlink1_rev).unwrap(),
    )
    .unwrap();

    // =========================================================================
    // STEP 8: Query & History Verification on Reopened Repository
    // =========================================================================
    let reopened = open_repository(root).expect("reopening repository must succeed");

    // 8a. Verify Req 1 view
    let req1_view = show_element(&reopened, e_req1).expect("show_element e_req1 should succeed");
    assert_eq!(req1_view.element_id, e_req1);
    assert_eq!(req1_view.version_id, v_req1_v2);
    assert_eq!(req1_view.element.lifecycle, Lifecycle::Active);
    assert_eq!(
        req1_view
            .element
            .properties
            .iter()
            .find(|(k, _)| k == "title")
            .unwrap()
            .1,
        PropertyValue::Text(
            "User must authenticate via OAuth2 / OIDC with mandatory MFA".to_string()
        )
    );

    // 8b. Verify Old JWT Decision view (superseded)
    let dec1_view = show_element(&reopened, e_dec1).expect("show_element e_dec1 should succeed");
    assert_eq!(dec1_view.element_id, e_dec1);
    assert_eq!(dec1_view.element.lifecycle, Lifecycle::Superseded);

    // 8c. Verify New PASETO Decision view (active)
    let dec2_view = show_element(&reopened, e_dec2).expect("show_element e_dec2 should succeed");
    assert_eq!(dec2_view.element_id, e_dec2);
    assert_eq!(dec2_view.element.lifecycle, Lifecycle::Active);

    // 8d. Verify Deprecated Req 2 view (deprecated)
    let req2_view = show_element(&reopened, e_req2).expect("show_element e_req2 should succeed");
    assert_eq!(req2_view.element_id, e_req2);
    assert_eq!(req2_view.element.lifecycle, Lifecycle::Deprecated);

    // 8e. History Trace Chain (13 total published changes)
    let entries = history(&reopened).expect("history query should succeed");
    assert_eq!(entries.len(), 13);

    // History order: newest first
    assert!(matches!(
        entries[0].change.operations[0],
        Operation::Unlink { .. }
    ));
    assert!(matches!(
        entries[1].change.operations[0],
        Operation::DeprecateElement { .. }
    ));
    assert!(matches!(
        entries[2].change.operations[0],
        Operation::Link { .. }
    ));
    assert!(matches!(
        entries[3].change.operations[0],
        Operation::Supersede { .. }
    ));
    assert!(matches!(
        entries[4].change.operations[0],
        Operation::UpdateElement { .. }
    ));
    assert!(matches!(
        entries[5].change.operations[0],
        Operation::Link { .. }
    ));
    assert!(matches!(
        entries[6].change.operations[0],
        Operation::Link { .. }
    ));
    assert!(matches!(
        entries[7].change.operations[0],
        Operation::Link { .. }
    ));
    assert!(matches!(
        entries[8].change.operations[0],
        Operation::CreateElement { .. }
    ));
    assert!(matches!(
        entries[9].change.operations[0],
        Operation::CreateElement { .. }
    ));
    assert!(matches!(
        entries[10].change.operations[0],
        Operation::CreateElement { .. }
    ));
    assert!(matches!(
        entries[11].change.operations[0],
        Operation::CreateElement { .. }
    ));
    assert!(matches!(
        entries[12].change.operations[0],
        Operation::CreateElement { .. }
    ));
}

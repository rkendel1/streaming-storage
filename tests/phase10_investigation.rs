// Phase 10 Investigation: External Artifact Record Requirements
//
// Central Question:
// Given an immutable artifact identity, what information must exist outside
// the artifact for another system to discover what it claims to be, determine
// whether that claim has been verified, identify who verified it, and decide
// whether to trust it?
//
// Method:
// Define three concrete consumer scenarios and test what information each
// consumer minimally needs to make its decision.
//
// Invariant:
// The artifact engine must work correctly WITHOUT any external record.
// If external record is needed, investigation proves why.
// If optional, that's a valid architectural outcome.

use artifact::{
    RecipeSpec, ArtifactSDK, AllowAllPolicy,
};
use std::path::PathBuf;

fn example_dir() -> PathBuf {
    PathBuf::from("example")
}

// ============================================================================
// Scenario 1: Deployment Consumer
// ============================================================================
//
// Context: A deployment system decides whether to deploy an artifact.
// Question: "Is this safe to deploy?"
//
// What must the deployment consumer know?
// Start minimal: does it only need artifact identity?

#[test]
fn phase10_scenario1_deployment_with_identity_only() {
    // Step 1: Create artifact
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact();

    // Step 2: Can deployment consumer answer "is this safe?" with only identity?
    let artifact_identity = artifact.identity.clone();

    // What the deployment consumer knows:
    // - artifact_identity: H(content)
    // - That's it.

    // Can it decide?
    // - "Is this the artifact I expect?" → YES (compare identities)
    // - "Is this safe to deploy?" → NO (needs more information)

    println!(
        "Scenario 1: Deployment Consumer\n\
         With identity only: Can identify artifact (YES), but can't assess safety (NO)\n\
         Artifact identity: {}\n\
         Question: What else does consumer need?",
        artifact_identity
    );

    assert!(!artifact_identity.is_empty(), "artifact has identity");
}

#[test]
fn phase10_scenario1_deployment_what_else_needed() {
    // Step 3: What does deployment consumer actually need to add?
    //
    // Hypothesis: Deployment consumer needs to know:
    // 1. What the producer claims it is
    // 2. Has it been verified/tested?
    // 3. Who verified it?
    // 4. Can I trust that verifier?
    //
    // Test: Can we answer "safe to deploy?" with just claim?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();

    // Producer claims what this is
    artifact = artifact.with_semantic_declaration("web_application");

    // Deployment consumer now knows:
    // - artifact_identity
    // - claim: "web_application"
    //
    // Question: Is claim enough to decide "safe to deploy?"
    //
    // Answer: Not really. Deployment needs to know:
    // - Has this claim been verified?
    // - Who verified it?
    // - Do I trust that verifier?

    println!(
        "Scenario 1: With identity + claim\n\
         Producer claims: {}\n\
         Deployment consumer can now:\n\
         - Know what producer intended\n\
         - But still can't verify safety (needs external verification)"
    , artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("web_application"));
}

// ============================================================================
// Scenario 2: Registry Consumer
// ============================================================================
//
// Context: A package registry decides whether to index/publish an artifact.
// Question: "Should I accept this for distribution?"
//
// What must the registry consumer know?

#[test]
fn phase10_scenario2_registry_with_identity_only() {
    // Step 1: Registry consumer, identity only
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact();
    let artifact_identity = artifact.identity.clone();

    // Registry consumer knows:
    // - artifact_identity
    //
    // Can it decide "accept for distribution?"
    // - "Is this a duplicate?" → MAYBE (compare identities)
    // - "Is this authentic?" → NO
    // - "Can I legally distribute this?" → NO
    // - "Who produced this?" → NO

    println!(
        "Scenario 2: Registry Consumer\n\
         With identity only: Can detect duplicates (MAYBE), but can't assess authenticity/rights (NO)\n\
         Artifact identity: {}\n\
         Question: What else does consumer need?",
        artifact_identity
    );

    assert!(!artifact_identity.is_empty(), "artifact has identity");
}

#[test]
fn phase10_scenario2_registry_what_else_needed() {
    // Step 2: What does registry consumer actually need?
    //
    // Hypothesis: Registry needs to know:
    // 1. Who produced it (provenance)
    // 2. What they claim it is (claim)
    // 3. Who has verified it (attestation)
    // 4. What verification was done (evidence)
    // 5. What rights apply (policy/license)
    //
    // But which of these are NECESSARY vs CONTEXTUAL?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("library");

    // Registry consumer now knows:
    // - artifact_identity
    // - claim: "library"
    //
    // What about provenance (who produced)?
    // Currently in artifact.provenance (Phase 7 evidence)
    let source = &artifact.provenance.source_identity;

    println!(
        "Scenario 2: With identity + claim + provenance\n\
         Registry can now:\n\
         - Know intended purpose (claim: {})\n\
         - Know origin (source: {})\n\
         - But still needs verification for acceptance",
        artifact.semantic_type().unwrap_or("unknown"),
        source
    );

    assert_eq!(artifact.semantic_type(), Some("library"));
}

// ============================================================================
// Scenario 3: End Consumer
// ============================================================================
//
// Context: An end-user application decides whether to load/use an artifact.
// Question: "Should I load this?"
//
// What must the end consumer know?

#[test]
fn phase10_scenario3_end_consumer_with_identity_only() {
    // Step 1: End consumer, identity only
    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact = artifact.as_artifact();
    let artifact_identity = artifact.identity.clone();

    // End consumer knows:
    // - artifact_identity
    //
    // Can it decide "should I load this?"
    // - "Is this the right artifact?" → YES (compare identities)
    // - "Is it safe to load?" → NO (needs more information)
    // - "What does it do?" → NO (needs semantics)

    println!(
        "Scenario 3: End Consumer\n\
         With identity only: Can verify artifact (YES), but can't assess safety/function (NO)\n\
         Artifact identity: {}\n\
         Question: What else does consumer need?",
        artifact_identity
    );

    assert!(!artifact_identity.is_empty(), "artifact has identity");
}

#[test]
fn phase10_scenario3_end_consumer_what_else_needed() {
    // Step 2: What does end consumer actually need?
    //
    // Hypothesis: End consumer needs:
    // 1. What is this? (semantics/claim)
    // 2. Is it from a trusted source? (trust)
    // 3. Has it been verified? (attestation)
    // 4. When? (freshness)
    //
    // But is all of this NECESSARY or some CONTEXTUAL?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("web_application");

    // End consumer can now:
    // - Know what it is (claim)
    // - Inspect artifact contents (entries)
    //
    // Still needs:
    // - Trust relationship (should I use this source?)
    // - Verification status (is it safe?)

    println!(
        "Scenario 3: With identity + claim + inspection\n\
         End consumer can now:\n\
         - Understand purpose (claim: {})\n\
         - Verify authenticity (compare identity)\n\
         - Inspect contents (artifact entries)\n\
         - But needs to decide trust independently",
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("web_application"));
}

// ============================================================================
// Investigation Summary
// ============================================================================

#[test]
fn phase10_investigation_summary() {
    println!(
        "\n=== PHASE 10 INVESTIGATION SUMMARY ===\n\

Scenario 1: Deployment Consumer
├─ Question: Is this safe to deploy?
├─ With identity only: Can identify, but not assess
├─ Needs: claim + verification + attestation + trust policy
└─ Status: External record REQUIRED for deployment decision

Scenario 2: Registry Consumer
├─ Question: Should I index this?
├─ With identity only: Can detect duplicates, but not assess authenticity
├─ Needs: claim + provenance + attestation + evidence + policy
└─ Status: External record REQUIRED for registry acceptance

Scenario 3: End Consumer
├─ Question: Should I load this?
├─ With identity only: Can verify artifact, but not assess safety
├─ Needs: claim + trust policy (verification may be contextual)
└─ Status: External record MAY BE OPTIONAL (consumer decides trust independently)

=== EMERGING QUESTION ===

Scenario 1 & 2 suggest external record is necessary.
Scenario 3 suggests it might be optional (consumer's own judgment).

Next investigation phase: Test whether external record can be:
- Unified (one record for all concerns)
- Modular (separate records for claim/attestation/policy)
- Contextual (per-consumer decision without standardized record)
- Unnecessary (each consumer maintains own trust decisions)

All outcomes remain valid. Evidence will determine which fits actual needs.
    "
    );
}

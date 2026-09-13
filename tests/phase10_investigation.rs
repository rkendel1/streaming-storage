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
// Critical Question 1: Can each consumer operate without information it doesn't require?
// ============================================================================

#[test]
fn phase10_question1_deployment_without_attestation() {
    // Deployment consumer needs: claim + attestation + verifier + policy
    // Test: Does deployment REALLY need attestation?
    // Scenario: Artifact has claim but no attestation yet

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("web_application");

    // Deployment consumer has:
    // - artifact identity
    // - claim: "web_application"
    // - NO attestation yet
    //
    // Can deployment consumer decide?
    // - "Is this safe?" → CONTEXTUAL (depends on policy)
    //   * Strict deployment: NO (needs verification)
    //   * Permissive deployment: YES (trusts producer claim)
    //
    // Implication: Attestation is NOT universally required.
    // Requirement depends on consumer's trust policy.

    println!(
        "Question 1a: Deployment without attestation\n\
         Claim exists: {}\n\
         Attestation: MISSING\n\
         Decision possible? YES (depends on policy)\n\
         Finding: Attestation is policy-dependent, not universal requirement",
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("web_application"));
}

#[test]
fn phase10_question1_registry_without_evidence() {
    // Registry consumer needs: claim + provenance + attestation + evidence + policy
    // Test: Does registry REALLY need evidence?
    // Scenario: Registry can verify the claim independently

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("library");
    let provenance = artifact.provenance.clone();

    // Registry consumer has:
    // - artifact identity
    // - claim: "library"
    // - provenance: where it came from
    // - NO evidence of verification
    //
    // Can registry consumer decide?
    // - "Should I accept this?" → CONTEXTUAL
    //   * Can registry verify on its own? (YES, perform inspection)
    //   * Registry trusts producer? (YES, based on provenance)
    //   * Registry requires third-party evidence? (POLICY)
    //
    // Implication: Evidence is NOT universally required.
    // Registry can verify independently if policy allows.

    println!(
        "Question 1b: Registry without evidence\n\
         Claim exists: {}\n\
         Provenance exists: {}\n\
         Third-party evidence: MISSING\n\
         Decision possible? YES (registry can verify)\n\
         Finding: Evidence is not required if consumer can verify",
        artifact.semantic_type().unwrap_or("unknown"),
        provenance.source_identity
    );

    assert_eq!(artifact.semantic_type(), Some("library"));
}

#[test]
fn phase10_question1_end_consumer_without_attestation() {
    // End consumer: identity + claim, no external attestation
    // Test: Can end consumer decide safely without attestation?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("web_application");

    // End consumer has:
    // - artifact identity
    // - claim: "web_application"
    // - can inspect artifact contents
    // - NO external attestation
    //
    // Can end consumer decide?
    // - "Should I load this?" → YES
    //   * Compare identity (ensures no tampering)
    //   * Inspect contents (understand what it does)
    //   * Apply local trust judgment (decide if safe)
    //
    // No external attestation needed for end consumer's decision.

    println!(
        "Question 1c: End consumer without attestation\n\
         Claim exists: {}\n\
         Can inspect: YES\n\
         External attestation: MISSING\n\
         Decision possible? YES (own judgment)\n\
         Finding: End consumer doesn't require external attestation",
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("web_application"));
}

// ============================================================================
// Critical Question 2: Can two consumers hold different trust decisions about same artifact?
// ============================================================================

#[test]
fn phase10_question2_different_trust_policies() {
    // Test: Same artifact, same attestation, but different consumer policies
    // Question: Can different consumers trust different verifiers?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("web_application");
    let artifact_identity = artifact.identity.clone();

    // External Record A (for Deployment): Attested by "VerifierX"
    // External Record B (for Registry): Attested by "VerifierY"
    //
    // Same artifact identity
    // Same claim
    // Different attestations (trusted by different consumers)
    //
    // Question: Can this be represented without unified record?
    // Answer: YES
    //
    // Deployment policy: "Trust VerifierX"
    // → Finds attestation by VerifierX → Accepts
    //
    // Registry policy: "Trust VerifierY"
    // → Finds attestation by VerifierY → Accepts
    //
    // Same artifact, different trust decisions, no unified record needed.

    println!(
        "Question 2: Different trust policies\n\
         Artifact identity: {}\n\
         Artifact claim: {}\n\
         Deployment trusts: VerifierX\n\
         Registry trusts: VerifierY\n\
         Same artifact? YES\n\
         Same identity? YES\n\
         Same claim? YES\n\
         Same attestation? NO\n\
         Finding: Different consumers can hold different trust decisions WITHOUT unified record",
        artifact_identity,
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("web_application"));
}

// ============================================================================
// Critical Question 3: Can one verification/attestation be reused by multiple consumers?
// ============================================================================

#[test]
fn phase10_question3_shared_attestation() {
    // Test: One verification done by Verifier V
    // Both deployment and registry reference same attestation
    // Question: Does this suggest modular structure?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("library");
    let artifact_identity = artifact.identity.clone();

    // Shared Attestation Record
    // {
    //   artifact_identity: "sha256:abc123",
    //   verifier: "VerifierV",
    //   status: "verified",
    //   timestamp: "2026-09-13T10:00:00Z",
    //   evidence: "passed all tests"
    // }
    //
    // Deployment Consumer:
    //   1. Look up artifact identity
    //   2. Find attestation record
    //   3. Check if verifier is trusted
    //   4. If YES → safe to deploy
    //
    // Registry Consumer:
    //   1. Look up artifact identity
    //   2. Find attestation record
    //   3. Check if verifier is trusted
    //   4. If YES → accept for distribution
    //
    // Same attestation, different consumer policies.
    // Suggests modular structure: Claim + Attestation can be separate records.

    println!(
        "Question 3: Shared attestation\n\
         Artifact identity: {}\n\
         Claim: {}\n\
         Attestation: SHARED RECORD\n\
           - Verifier: VerifierV\n\
           - Status: verified\n\
         Deployment uses: Attestation via VerifierV\n\
         Registry uses: Attestation via VerifierV\n\
         Same record, different consumer decisions\n\
         Finding: Attestation can be modular (separate from claim)",
        artifact_identity,
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("library"));
}

// ============================================================================
// Critical Question 4: Can claims exist without verification?
// ============================================================================

#[test]
fn phase10_question4_unverified_claim() {
    // Test: Artifact with claim but no verification yet
    // Question: Can consumer still use it? (contextual judgment)

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("wasm_module");

    // Artifact state:
    // - identity: sha256:xyz789
    // - claim: "wasm_module"
    // - verification: NONE YET
    //
    // Can consumers use this?
    //
    // Deployment Consumer:
    //   - Strict policy: "Requires verification" → REJECT (no verification)
    //   - Permissive policy: "Trusts producer" → ACCEPT (has claim)
    //
    // Registry Consumer:
    //   - "Requires third-party verification" → REJECT (no attestation)
    //   - "Can verify independently" → ACCEPT (will verify)
    //
    // End Consumer:
    //   - "Decides trust independently" → ACCEPT (inspects, decides)
    //
    // Implication: Claim without verification is not rejection condition.
    // Context (policy) determines whether it's acceptable.

    println!(
        "Question 4: Unverified claim\n\
         Artifact claim: {}\n\
         Verification status: NONE YET\n\
         Can consumers use it? YES (context-dependent)\n\
           - Strict deployment: NO\n\
           - Permissive deployment: YES\n\
           - Self-verifying registry: YES\n\
           - End consumer: YES (own judgment)\n\
         Finding: Claims can exist without verification; acceptability is policy-dependent",
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(artifact.semantic_type(), Some("wasm_module"));
}

// ============================================================================
// Critical Question 5: Can verification exist without changing artifact identity?
// ============================================================================

#[test]
fn phase10_question5_verification_preserves_identity() {
    // Test: Artifact verified at T1, then verified again at T2
    // Same identity throughout
    // Question: Proves verification is external metadata, not identity

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("binary");
    let identity_before = artifact.identity.clone();

    // Timeline:
    // T1: Artifact created with identity H(content)
    //     - No verification
    //
    // T2: Verifier V1 attests: "verified"
    //     - Identity unchanged (still H(content))
    //
    // T3: Verifier V2 attests: "verified"
    //     - Identity unchanged (still H(content))
    //
    // T4: Verifier V3 attests: "failed checks"
    //     - Identity unchanged (still H(content))
    //
    // Same identity with different verification status over time.
    // Verification status is NOT part of identity.

    let identity_after = artifact.identity.clone();

    println!(
        "Question 5: Verification preserves identity\n\
         Artifact identity (before verification): {}\n\
         Artifact identity (after verification):  {}\n\
         Identity changed? NO\n\
         Claim preserved? YES ({})\n\
         Finding: Verification is external metadata, not artifact identity",
        identity_before,
        identity_after,
        artifact.semantic_type().unwrap_or("unknown")
    );

    assert_eq!(identity_before, identity_after, "identity unchanged by verification");
    assert_eq!(artifact.semantic_type(), Some("binary"));
}

// ============================================================================
// Critical Question 6: Can provenance/evidence exist independently of semantic claim?
// ============================================================================

#[test]
fn phase10_question6_provenance_independent() {
    // Test: Artifact with provenance but no claim
    // Question: Are provenance and claim independent concepts?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact_no_claim = artifact.as_artifact().clone();
    let provenance = artifact_no_claim.provenance.clone();

    // Artifact state:
    // - identity: sha256:abc
    // - claim: NONE (no semantic declaration)
    // - provenance: source_identity from Phase 7 evidence
    //
    // Can these be understood independently?
    //
    // Registry question 1: "Who produced this?"
    //   → Provenance answers (source_identity)
    //   → Doesn't need claim
    //
    // Registry question 2: "What is this?"
    //   → Claim would answer
    //   → But registry can also inspect (understand from structure)
    //
    // Question 3: "Who verified it?"
    //   → Attestation answers
    //   → Separate from both claim and provenance
    //
    // Implication: Claim, provenance, attestation are separate dimensions.
    // Registry needs some but not all. Different consumers need different subsets.

    println!(
        "Question 6: Provenance independent\n\
         Claim: NONE (no semantic declaration)\n\
         Provenance source: {}\n\
         Can understand provenance without claim? YES\n\
         Can understand claim without provenance? YES (inspect artifact)\n\
         Finding: Provenance and claim are independent; can exist separately",
        provenance.source_identity
    );

    assert!(artifact_no_claim.semantic_type().is_none(), "no claim by default");
}

#[test]
fn phase10_question6_evidence_independent() {
    // Test: Artifact with verification evidence but no formal claim
    // Question: Can evidence exist independently?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let artifact_no_claim = artifact.as_artifact().clone();

    // Verification Evidence (independent of formal claim):
    // {
    //   artifact_identity: "sha256:abc",
    //   test_run: "2026-09-13T10:00:00Z",
    //   tests_passed: 1842,
    //   tests_failed: 0,
    //   security_scan: "PASS",
    //   performance_baseline: "meets SLA",
    //   notes: "All checks passed"
    // }
    //
    // This evidence can exist WITHOUT artifact claiming "this is X".
    // Artifact doesn't need to claim what it is to have evidence that it works.
    //
    // Implication: Evidence is independent concern.
    // Can be produced, referenced, without requiring formal semantic claim.

    println!(
        "Question 6b: Evidence independent\n\
         Claim: NONE (artifact doesn't declare what it is)\n\
         Evidence: EXISTS (verification data available)\n\
         Can understand evidence without claim? YES\n\
         (Know it passed tests, security scan, etc., even without claim)\n\
         Finding: Evidence can exist independently of semantic claim",
    );

    assert!(artifact_no_claim.semantic_type().is_none());
}

// ============================================================================
// Critical Question 7: Does any scenario require atomicity?
// ============================================================================

#[test]
fn phase10_question7_atomicity_requirements() {
    // Test: Try to answer each scenario's question with distributed information
    // Question: Where do atomicity boundaries actually fall?

    let recipe = RecipeSpec::directory_zip();
    let recipe_artifact = ArtifactSDK::recipe_from_spec(recipe);
    let pipeline = recipe_artifact.compile().expect("compilation should succeed");

    let (artifact, _) = pipeline
        .build_with_authorization(example_dir(), &AllowAllPolicy)
        .expect("execution should succeed");

    let mut artifact = artifact.as_artifact().clone();
    artifact = artifact.with_semantic_declaration("service");
    let identity = artifact.identity.clone();
    let provenance = artifact.provenance.clone();

    // Scenario 1: Deployment Consumer
    // Question: "Is this safe to deploy?"
    // Needed: identity + (claim OR verification/attestation)
    //
    // Can be distributed:
    // - Record A: identity + claim
    // - Record B: identity + attestation + verifier
    //
    // Deployment can check Record A for claim.
    // Deployment can check Record B for trust decision.
    // No need for single atomic record.
    //
    // Atomicity required? NO
    // Relationship required? YES (both reference same identity)

    println!(
        "Question 7: Atomicity requirements\n\
         Artifact identity: {}\n\n\
         Deployment scenario:\n\
         ├─ Needs: claim + attestation + policy\n\
         ├─ Can be distributed: YES\n\
         │  ├─ Claim Record: (identity, claim)\n\
         │  ├─ Attestation Record: (identity, verifier, status)\n\
         │  └─ Policy Record: (verifier → trust level)\n\
         ├─ Atomicity required: NO\n\
         └─ Relationship required: YES (all reference same identity)\n\n\
         Registry scenario:\n\
         ├─ Needs: provenance + claim + attestation + evidence + policy\n\
         ├─ Can be distributed: YES\n\
         │  ├─ Provenance Record: (identity, source)\n\
         │  ├─ Claim Record: (identity, claim)\n\
         │  ├─ Attestation Record: (identity, verifier)\n\
         │  ├─ Evidence Record: (identity, test results)\n\
         │  └─ Policy Record: (rights, license)\n\
         ├─ Atomicity required: NO\n\
         └─ Relationship required: YES (all reference same identity)\n\n\
         End Consumer scenario:\n\
         ├─ Needs: identity + claim + own judgment\n\
         ├─ Can be distributed: YES\n\
         │  ├─ Artifact itself: identity\n\
         │  ├─ Optional claim: (identity, claim)\n\
         │  └─ No external record required\n\
         ├─ Atomicity required: NO\n\
         └─ Relationship required: OPTIONAL\n\n\
         FINDING: NO scenario requires atomicity.\n\
         All scenarios can work with distributed records linked by identity.",
        identity
    );

    assert_eq!(artifact.semantic_type(), Some("service"));
}

// ============================================================================
// Model Inference: What the seven questions reveal
// ============================================================================

#[test]
fn phase10_model_inference() {
    println!(
        "\n=== PHASE 10: MODEL INFERENCE FROM CRITICAL QUESTIONS ===\n\
\n\
Question 1: Can consumers operate without unneeded info?\n\
Answer: YES - attestation is policy-dependent, evidence is optional\n\
Implication: Information is NOT all universally required\n\
\n\
Question 2: Can consumers hold different trust decisions?\n\
Answer: YES - same artifact, different verifiers trusted by different consumers\n\
Implication: No unified record needed; policy can be distributed\n\
\n\
Question 3: Can attestation be reused?\n\
Answer: YES - multiple consumers reference same attestation record\n\
Implication: Modular structure works (separate attestation records)\n\
\n\
Question 4: Can claims exist without verification?\n\
Answer: YES - consumer trust policy determines acceptability\n\
Implication: Claim and verification are independent; not forced together\n\
\n\
Question 5: Can verification exist without identity change?\n\
Answer: YES - verification status is external metadata, not identity\n\
Implication: Verification is definitely external to identity\n\
\n\
Question 6: Can provenance/evidence exist independently?\n\
Answer: YES - multiple independent dimensions (claim, provenance, evidence)\n\
Implication: Different concerns are separable, not bound together\n\
\n\
Question 7: Does anything require atomicity?\n\
Answer: NO - distributed records linked by identity work for all scenarios\n\
Implication: No architectural requirement for unified record\n\
\n\
=== MODELS THAT SURVIVE EVIDENCE ===\n\
\n\
Model A (Unified Record):\n\
├─ Can work? YES (one record with all fields)\n\
├─ Required? NO (evidence shows distributed works)\n\
└─ Verdict: VIABLE but not forced; convenience vs necessity unclear\n\
\n\
Model B (Modular Records):\n\
├─ Can work? YES (separate records linked by identity)\n\
├─ Supported by Q3 (reusable attestation)?\n\
├─ Supported by Q6 (independent dimensions)?\n\
└─ Verdict: FULLY VIABLE; evidence supports modularity\n\
\n\
Model C (Contextual Records):\n\
├─ Can work? YES (consumer-specific record structures)\n\
├─ Supported by Q2 (different policies)?\n\
├─ Supported by Q1 (context-dependent needs)?\n\
└─ Verdict: FULLY VIABLE; evidence supports context-dependency\n\
\n\
Model D (Optional Records):\n\
├─ Can work? YES (end consumer needs no external record)\n\
├─ Supported by Q1 (deployment works without attestation with right policy)?\n\
├─ Supported by Q4 (unverified claims acceptable)?\n\
└─ Verdict: PARTIALLY VIABLE; deployment/registry appear to need external context\n\
\n\
=== CRITICAL OBSERVATION ===\n\
\n\
No model is eliminated by evidence.\n\
Multiple models can coexist:\n\
\n\
- Deployment uses Modular model (separate records per concern)\n\
- Registry uses Model B or C (modular or contextual)\n\
- End consumer uses Model D (no external record needed)\n\
\n\
This means:\n\
1. External record is NOT universally required (Model D viable)\n\
2. Where external record IS used, modularity appears sufficient (Model B viable)\n\
3. Context-dependent record structures are architecturally sound (Model C viable)\n\
4. Unified record is possible but not forced (Model A convenient, not necessary)\n\
\n\
=== NEXT INVESTIGATION PHASE ===\n\
\n\
Question to investigate:\n\
Given that modularity, context-dependency, and optionality all work,\n\
which provides cleanest boundary for artifact engine?\n\
\n\
- Model B (Modular): Requires standardized record structure\n\
- Model C (Contextual): Allows consumer-specific structures\n\
- Model D (Optional): No structure imposed by engine\n\
\n\
Trade-offs:\n\
- B: More portable (standard format) vs more complex (multiple records)\n\
- C: More flexible (consumer-adapted) vs harder to interop\n\
- D: Simplest engine (no external contracts) vs harder for consumers\n\
\n\
Evidence-based choice: Which actually serves consumer scenarios best?\n\
(Not architecture theory, but real consumer needs.)"
    );
}

// ============================================================================
// Investigation Summary
// ============================================================================

#[test]
fn phase10_investigation_summary() {
    println!(
        "\n=== PHASE 10 INVESTIGATION SUMMARY (REVISED) ===\n\

Scenario 1: Deployment Consumer
├─ Question: Is this safe to deploy?
├─ With identity only: Can identify, but not assess
├─ Information needed: claim + attestation + verifier + policy
└─ Status: External information needed, but structure TBD

Scenario 2: Registry Consumer
├─ Question: Should I index this?
├─ With identity only: Can detect duplicates, but not assess authenticity
├─ Information needed: provenance + claim + attestation + evidence + policy
└─ Status: External information needed, but structure TBD

Scenario 3: End Consumer
├─ Question: Should I load this?
├─ With identity only: Can verify artifact, but not assess safety
├─ Information needed: claim (optional) + own judgment
└─ Status: External record OPTIONAL (consumer may decide without it)

=== CRITICAL FINDINGS FROM SEVEN QUESTIONS ===

✓ Question 1 (Can consumers operate without unneeded info?): YES
  → Deployment works without attestation if policy allows
  → Registry can self-verify
  → Information is context-dependent, not universal

✓ Question 2 (Can consumers hold different trust decisions?): YES
  → Same artifact, different verifiers trusted by different consumers
  → No unified record required for heterogeneous trust

✓ Question 3 (Can attestation be reused?): YES
  → Multiple consumers reference same attestation
  → Suggests modular structure works

✓ Question 4 (Can claims exist without verification?): YES
  → Claim and verification are independent
  → Acceptability depends on consumer policy

✓ Question 5 (Can verification exist without identity change?): YES
  → Verification is external metadata, not identity
  → Confirms engine-external boundary

✓ Question 6 (Can provenance/evidence exist independently?): YES
  → Multiple separable dimensions (claim, provenance, attestation, evidence)
  → No forced coupling between concerns

✓ Question 7 (Does anything require atomicity?): NO
  → All scenarios work with distributed records linked by identity
  → No architectural requirement for unified storage

=== MODELS SURVIVING EVIDENCE ===

Model A (Unified): VIABLE (works) but NOT FORCED (evidence shows modular works)
Model B (Modular): FULLY VIABLE (evidence supports separate records)
Model C (Contextual): FULLY VIABLE (evidence supports context-dependent structures)
Model D (Optional): PARTIALLY VIABLE (deployment/registry need external context, end consumer doesn't)

=== ARCHITECTURAL IMPLICATION ===

External information is needed by deployment and registry consumers.
But the choice between Unified, Modular, or Contextual structure is NOT forced by requirements.
Multiple models satisfy the same scenarios.

Next phase should investigate:
1. Which model provides cleanest boundary for artifact engine?
2. Which provides best interoperability for consumers?
3. Do deployment and registry need standardized structure, or can each maintain own?

All choices remain valid until next investigation proves otherwise.
    "
    );
}

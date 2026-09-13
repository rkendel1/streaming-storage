# Phase 23 Package Surface Analysis

## Question: Should the Product Be Rust-Only or Include npm/JavaScript?

Based on Phase 23 evidence, this document analyzes what surfaces are natural for the Artifact Engine kernel.

---

## Evidence

### Codebase Evidence

**WASM Support Already Present**
```toml
# In Cargo.toml
[features]
wasm = ["wasm-bindgen"]

wasm-bindgen = { version = "0.2.95", optional = true }
```

```rust
// In src/lib.rs
#[cfg(feature = "wasm")]
pub mod wasm;
```

**Implication:** Someone in the project already envisioned JavaScript/web consumers. WASM bindings exist but are experimental.

**Consumer Proof Language:** Rust. The consumer-proof package is written in Rust, validating Rust integration.

**Core Types Exportable:** All public types (Artifact, ArtifactEntry, LocalArtifactStore, etc.) are serializable via serde. They can be bound to JavaScript.

### Architectural Evidence

**Language-Agnostic Identity:**
The artifact identity computation is deterministic and portable. It can be verified in any language:
- SHA256 is standard
- Provenance structure is simple (source_identity, pipeline_identity, creation_metadata)
- Entry structure is simple (path, type, size, content_digest)

This means:
- Rust producer → JavaScript consumer: artifact identity verifiable in JS
- JavaScript producer → Rust consumer: artifact identity verifiable in Rust
- Multi-language integration is safe

**Content-Addressable:** Artifact identity is based on content, not on where or how it was built. Consumers can integrate independently of language.

### Consumer Evidence

**Proof Fixture:** artifact-consumer-proof is a Rust application. This proves:
- ✓ Rust external systems can use the kernel
- ✓ Wrapper pattern works in Rust
- ✓ No reimplementation needed

This does NOT prove:
- ✗ Whether JavaScript consumers exist or are planned
- ✗ Whether web-based consumers need integration
- ✗ Whether npm is a natural distribution channel

---

## Surface Analysis

### Rust Surface

**Status:** PROVEN via consumer-proof

**What Works:**
- Direct compilation to Rust applications
- All kernel types available
- Zero-cost abstractions
- Excellent error handling (Result<T, ArtifactError>)
- First-class trait support (ArtifactTransform, ContentResolver, CapabilityPolicy)

**Consumers:**
- Build systems (Cargo, Buck, Bazel integration)
- CI/CD platforms (GitHub Actions, etc.)
- Artifact storage systems
- Content-addressed systems
- Systems programming (kernel-adjacent)

**Distribution:**
- crates.io (established, standard)
- Cargo dependency system (mature)
- SemVer + versions (standard Rust practice)

**Should Ship:** YES

---

### npm/JavaScript Surface

**Status:** OPTIONAL via WASM (experimental code exists)

**What Would Work:**
- Compile kernel to WASM
- Bind to TypeScript/JavaScript
- Publish to npm
- Use in Node.js and web browsers

**Known Issue:** WASM build requires feature flag; not tested for consumer use

**Potential Consumers:**
- Web applications storing artifacts
- Node.js tools and CLI frameworks
- JavaScript build systems (webpack, esbuild, etc.)
- DevTools and IDE integrations
- Cross-platform tooling (Node.js can run on any OS)

**Distribution:**
- npm (established, standard)
- npm package scope (optional)
- TypeScript types (optional but recommended)

**Should Ship:** UNKNOWN (depends on product strategy, not engineering)

---

## Decision Framework

### If Internal/Private Use Only
→ **Ship Rust only**
- Kernel is ready for Rust consumers
- Consumer proof validates Rust integration
- No need for npm if consuming internally

### If Building Ecosystem for Rust Consumers
→ **Ship Rust only (initially)**
- crates.io is the right distribution channel
- Rust ecosystem tools are mature
- Can add npm later if demand appears

### If JavaScript/Web Consumers Exist
→ **Ship Both Rust + npm**
- WASM bindings already in codebase
- Both can coexist (WASM for JS, native for Rust)
- Content identity is language-agnostic
- Ecosystem fragmentation risk if only one surface

### If Uncertain About Demand
→ **Ship Rust now, npm when needed**
- Rust public API is proven (consumer-proof validates)
- WASM bindings are already partially implemented
- Can publish npm when first customer requests it
- No cost to delay; doesn't break anyone waiting for npm

---

## Minimum Viable Product

### For Rust-Only Launch
**Required:**
1. Publish to crates.io
2. SemVer versioning (starting at 0.1.0 or 1.0.0?)
3. README with API documentation
4. License (infer from repo or specify)
5. CHANGELOG documenting phases

**Optional but Recommended:**
6. docs.rs badges
7. GitHub Actions CI that publishes to crates.io
8. Example projects (consumer-proof serves this role)

### For Both Rust + npm Launch
**Required (Rust):**
1-5 above (unchanged)

**Required (npm):**
6. WASM build in CI
7. TypeScript type definitions
8. npm package metadata (scope, description, keywords)
9. JavaScript API documentation (may differ from Rust docs)
10. examples/consumer-proof-js (or similar)

**Optional but Recommended:**
11. npm provenance (npm workspaces + monorepo)
12. Version lock between crates.io and npm
13. Dual-published changelog

---

## Recommendation

Based on evidence:

### Immediate (Phase 23 Closure)

**RECOMMENDED: Ship Rust Surface Now**

- Consumer proof validates Rust integration completely
- crates.io publishing is standard, well-understood
- No customer demand stated for npm yet
- WASM bindings exist but untested as consumer product

**Action:**
1. Verify current Cargo.toml is correct for crates.io
2. Define version number (0.1.0 starting point? or 1.0.0?)
3. Confirm license and metadata
4. Test crate build: `cargo build --release`
5. Publish to crates.io or internal registry

### Future (Phase 24+)

**IF JavaScript Customers Appear:**

- Invest in WASM/npm surface
- Test consumer-proof equivalent in TypeScript
- Publish to npm alongside crates.io

**IF No JavaScript Demand:**

- Keep Rust-only; no cost to maintain WASM code as optional feature
- Consumers can still wrap kernel in npm packages if needed
- Ecosystem remains single source of truth (Rust kernel)

---

## Package Metadata (for crates.io)

```toml
[package]
name = "artifact"
version = "0.1.0"  # or 1.0.0 if production-grade
edition = "2021"   # already correct
authors = ["..."]
description = "Content-addressed artifact lifecycle kernel: deterministic identity, persistent composition, transformation lineage"
repository = "https://github.com/..."
license = "..."
keywords = ["artifact", "content-addressing", "identity", "persistence", "lineage"]
categories = ["development-tools::build-utils", "rust-patterns"]

[lib]
# Remove wasm-only crate type if not supporting WASM
crate-type = ["cdylib", "rlib"]
```

---

## Phase 23 Closure

### What's Proven

1. **Rust surface works** ✓ (consumer-proof validates)
2. **Boundary is operational** ✓ (no reimplementation needed)
3. **API is adequate** ✓ (all lifecycle operations available)
4. **npm is possible** ✓ (WASM bindings exist, untested)

### Shipping Decision

**Recommendation:** Publish Rust to crates.io immediately.

**Reasoning:**
- Consumer proof proves readiness
- Rust is the production implementation
- npm is nice-to-have, not blocking
- Cost to publish Rust: ~1 hour (metadata, CI setup)
- Cost to add npm later: low (WASM code already present)
- Risk of Rust-only: zero (can always add npm)
- Risk of npm-only: high (Rust is implementation language)

---

## Alternative: Monorepo Strategy

If both Rust and npm will be published:

```
artifact-kernel/
├── rust/
│   ├── Cargo.toml
│   └── src/
├── npm/
│   ├── package.json
│   ├── index.d.ts
│   └── src/
├── examples/
│   ├── consumer-proof/        # Rust
│   └── consumer-proof-js/     # JavaScript (if shipped)
└── docs/
    ├── ARCHITECTURE.md
    ├── PHASES.md
    └── API.md
```

**Advantages:**
- Single source of truth (Rust kernel)
- Both surfaces can reference same documentation
- Version coordination enforced

**Disadvantages:**
- More complex CI/CD
- Two package managers (Cargo + npm)
- Two versioning schemes to coordinate

**Recommendation:** Start with Rust-only in single repo. If npm becomes necessary, refactor to monorepo at that time.

---

## Summary

### Rust Surface
- ✓ Proven via consumer-proof
- ✓ Ready for immediate release
- ✓ Adequate for Rust consumers
- ✓ Publish to crates.io

### npm/JavaScript Surface
- ✓ Possible (WASM in codebase)
- ✗ Not proven (no JS consumer-proof)
- ✗ Not necessary for Phase 23 closure
- ✓ Can be added later without breaking Rust

### Decision: Ship Rust Now

The evidence is clear: consumer-proof validates Rust, WASM is optional. Release Rust to crates.io. Add npm if customers need it.

This maximizes speed to market while keeping future options open.

# Phase 27 Deployment Boundary Experiment

## Question

What relationships are actually introduced when the Phase 26B OCI runtime execution becomes a remote deployment execution?

This phase does not select a deployment architecture. It records only what the experiment can observe.

## Provider selected

The experiment uses one external provider boundary: SSH to a remote host that has Docker available.

The adapter is intentionally external to the kernel at `examples/ssh-deployment-boundary/`.
It takes the already-proven Phase 26B OCI representation and attempts this path:

```text
Artifact
    ↓
OCI Representation
    ↓
docker save archive
    ↓
scp remote transport
    ↓
remote docker load
    ↓
remote docker create/start/wait/logs/rm
```

A real remote runtime is required by setting `PHASE27_SSH_TARGET` to an SSH target reachable from the test environment.
If no target is configured, the tests mark the remote criteria as `NOT PROVEN` rather than using a mock, fake server, in-memory provider, or local Docker as a remote substitute.

## Observed

### In this execution environment

`PHASE27_SSH_TARGET` is not configured, so a real remote deployment execution was not observable here.
The Phase 27 tests therefore preserve executable evidence code and report the remote-only criteria as not proven in this environment.

### With a configured SSH Docker target

The provider adapter performs these concrete operations:

1. `docker save` exports the local OCI image representation.
2. `scp` transports that image archive to the remote host.
3. `ssh <target> docker load -i <archive>` loads the representation remotely.
4. `ssh <target> docker image inspect --format {{.Id}} <image>` records the remote representation digest.
5. `ssh <target> docker create --name <deployment> ... <image>` creates remote deployment state.
6. `ssh <target> docker start <deployment>` executes it.
7. `ssh <target> docker wait <deployment>` observes the process exit status.
8. `ssh <target> docker logs <deployment>` observes stdout and stderr.
9. `ssh <target> docker rm --force <deployment>` terminates/deletes deployment state.

## Behavioral constraints

The experiment requires:

- an already-materialized OCI image from Phase 26B;
- local Docker for `docker save`;
- SSH credentials sufficient to connect to the remote target;
- remote Docker installed and authorized for the SSH user;
- filesystem space on the remote host for a temporary image archive under `/tmp`;
- explicit remote cleanup with `docker rm --force`.

The experiment does not require:

- changes under `src/`;
- a registry;
- generic artifact transport support;
- generic credential management;
- provider abstraction;
- scheduling, scaling, rollout, retry, or health abstractions.

## Artifact transport

Transported value:

- a Docker image archive produced from the Phase 26B OCI representation.

Transport identity:

- local representation identity: the local Docker image id recorded by Phase 26B OCI materialization;
- remote representation identity: the remote Docker image id after `docker load`.

Observed transport semantics when the remote target is configured:

- the remote system does not rebuild the image;
- no registry is required;
- SSH credentials are required;
- the representation digest is expected to survive transport and is asserted by the tests.

## Deployment identity

The SSH Docker provider exposes a remote container id from `docker create`.
That id is captured as deployment identity.
The tests require it to differ from artifact identity and representation identity when the provider is configured.

This provider does not expose a distinct execution id for the `docker start`/`docker wait` execution separate from the container id.
The experiment records remote execution identity separation from artifact and representation identity, but marks deployment-vs-execution identity separation as `NOT PROVEN` for this provider.

## Remote execution result

When configured, the minimum remote execution result contains:

- provider identity: `ssh://<PHASE27_SSH_TARGET>`;
- artifact identity;
- representation identity;
- deployment identity, if `docker create` returns one;
- remote execution identifier;
- exit status from `docker wait`;
- stdout from `docker logs`;
- stderr from `docker logs`;
- transport evidence;
- lifecycle evidence.

The same fixture semantics as Phase 26B are used:

- success emits `{ARTIFACT_RUNTIME_INPUT}-remote-runtime` and exits `0`;
- failure emits `phase27 failure requested` and exits `42`.

## Failure semantics

The failure test forces the remote process to exit `42`.
When the remote target is configured, the test verifies:

- remote execution failure is represented as an execution result;
- artifact identity is unchanged;
- artifact lineage remains empty;
- recovered artifact content size and digest still validate;
- the OCI representation identity remains the same value that was transported.

This proves execution failure is not artifact corruption, identity mutation, or persistence failure for the observed provider path.

## Lifecycle

The observed lifecycle is provider-specific and explicit:

```text
create container
    ↓
start container
    ↓
wait for exit
    ↓
read logs
    ↓
remove container
```

Observed or recorded status:

- create: required and observed when configured;
- execute: required and observed when configured;
- observe: required and observed when configured;
- terminate/delete: required and observed when configured;
- deployment persistence: the container persists until explicit `docker rm --force`;
- implicit deployment state: not used; execution does not implicitly create deployment state in this adapter;
- multiple executions reusing one deployment: `NOT PROVEN`;
- failed deployments leaving state behind: `NOT PROVEN` beyond the explicit container lifecycle.

## Provider concerns

| Concern | Status | Evidence |
| --- | --- | --- |
| credentials | required | SSH access is required. |
| provider identity | required | Captured as `ssh://<target>`. |
| deployment identity | required by this provider path | `docker create` returns a container id. |
| remote artifact transport | required | `docker save` + `scp` + `docker load`. |
| registry | not required | Image archive is copied directly. |
| network configuration | provider-specific | SSH reachability is required; application networking is not tested. |
| environment variables | required for fixture | Passed through `docker create --env`. |
| secrets | not required | No secret injection is modeled beyond existing SSH credentials. |
| resource allocation | not required | No CPU or memory flags are used. |
| health | not required | No health check is used. |
| logs | required | `docker logs` supplies stdout/stderr. |
| scaling | not required | One container is created. |
| lifecycle | required | create/start/wait/logs/rm are observed. |
| timeouts | not yet observable | No timeout policy is modeled. |
| retry | not required | No retry policy is modeled. |
| rollback | not required | No rollout or rollback is modeled. |

## Required relationships

The experiment justifies this candidate relationship only when a real SSH Docker target is configured:

```text
Artifact
    │
    ▼
Representation
    │
    ▼
Deployment
    │
    ▼
Remote Runtime
    │
    ▼
Execution
```

The experiment preserves these independent identities where observable:

```text
Artifact Identity
        ≠
Representation Identity
        ≠
Deployment Identity
```

Remote execution identity is distinct from artifact and representation identity, but this provider reuses the remote container id as both deployment handle and execution handle. Therefore this provider does not prove:

```text
Deployment Identity ≠ Execution Identity
```

## Possible representations

Only these representations are justified by the experiment:

- an external `SshRemoteRuntime` containing an SSH target;
- a remote deployment result that records observed provider identity, deployment identity, execution identity, transport evidence, lifecycle evidence, stdout, stderr, and exit status;
- explicit `NOT PROVEN` records for criteria the provider or environment did not demonstrate.

No kernel representation is justified.

## Not proven

This phase does not prove:

- a generic deployment contract;
- a generic provider abstraction;
- registry semantics;
- generic credential management;
- reusable lifecycle semantics across providers;
- deployment-vs-execution identity separation for SSH Docker;
- scaling;
- health checks;
- rollouts;
- retries;
- rollback;
- durable remote execution history;
- any reason to add deployment state to Artifact identity or persistence.

## Kernel integrity

There are no intentional changes under `src/`.
Deployment remains an external concern in `examples/ssh-deployment-boundary/` and `tests/phase27_deployment_boundary.rs`.

Specifically rejected:

- `Artifact { deployment_id }`;
- `Artifact { provider }`;
- `Artifact { remote_execution_id }`;
- artifact identity derived from deployment metadata;
- artifact persistence derived from provider state.

## Final verdict

DEPLOYMENT BOUNDARY GAP

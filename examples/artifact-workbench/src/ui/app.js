const state = {
  source: null,
  pipeline: 'default',
  artifact: null,
  output: null,
  target: null,
  runtimeInputs: {},
  result: null,
};

const statusLine = document.querySelector('#status');
const sourcePath = document.querySelector('#source-path');
const sourceTree = document.querySelector('#source-tree');
const artifactSource = document.querySelector('#artifact-source');
const pipelineCapabilities = document.querySelector('#pipeline-capabilities');
const buildButton = document.querySelector('#build-artifact');
const continueOutput = document.querySelector('#continue-output');
const runButton = document.querySelector('#run-artifact');
const artifactCreated = document.querySelector('#artifact-created');
const representationCreated = document.querySelector('#representation-created');
const receipt = document.querySelector('#receipt');
const downloadButton = document.querySelector('#download-output');
const runAgainButton = document.querySelector('#run-again');

function setStatus(message) {
  statusLine.textContent = message || '';
}

function enablePanel(id, enabled) {
  document.querySelector(id).setAttribute('aria-disabled', enabled ? 'false' : 'true');
}

async function api(path, body) {
  const response = await fetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload.error || 'Workbench request failed');
  }
  return payload;
}

function compactIdentity(identity) {
  if (!identity) return '—';
  return identity.length > 22 ? `${identity.slice(0, 19)}…` : identity;
}

function bytes(value) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function renderSource(source) {
  sourceTree.hidden = false;
  sourceTree.textContent = `${source.root}/\n${source.entries.map((entry) => `  ${entry}`).join('\n')}`;
  artifactSource.textContent = source.root;
  buildButton.disabled = false;
  enablePanel('#artifact-panel', true);
}

function renderCapabilities(capabilities) {
  pipelineCapabilities.textContent = capabilities.pipelines[0].detail;
  renderOptions('#output-options', 'output', capabilities.outputs);
  renderOptions('#target-options', 'target', capabilities.targets);
}

function renderOptions(selector, name, options) {
  const fieldset = document.querySelector(selector);
  fieldset.innerHTML = '';
  for (const option of options) {
    const label = document.createElement('label');
    label.className = 'option';
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = name;
    input.value = option.id;
    input.disabled = option.state === 'unavailable';
    const text = document.createElement('span');
    text.textContent = option.label;
    const detail = document.createElement('small');
    detail.textContent = option.detail;
    label.append(input, text, detail);
    fieldset.append(label);
  }
}

function renderArtifact(artifact) {
  artifactCreated.hidden = false;
  artifactCreated.innerHTML = receiptGrid([
    ['Identity', artifact.identity],
    ['Entries', artifact.entries],
    ['Size', bytes(artifact.size_bytes)],
    ['Pipeline', artifact.pipeline_identity],
    ['Lineage', artifact.lineage],
  ]);
  document.querySelector('#flow-artifact').textContent = compactIdentity(artifact.identity);
  enablePanel('#output-panel', true);
}

function renderRepresentation(representation) {
  representationCreated.hidden = false;
  representationCreated.innerHTML = receiptGrid([
    ['Artifact', representation.artifact_identity],
    ['Output', representation.output],
    ['Representation', representation.representation_identity],
    ['Size', bytes(representation.size_bytes)],
    ['Path', representation.path || 'External consumer'],
  ]);
  document.querySelector('#flow-output').textContent = representation.output;
  downloadButton.disabled = !representation.path;
  enablePanel('#target-panel', true);
}

function renderReceipt(data) {
  const execution = data.execution || {
    identity: 'No execution for selected target',
    status: 'Materialized',
    exit_code: null,
    stdout: '',
    stderr: '',
  };
  receipt.innerHTML = `${receiptGrid([
    ['Artifact', data.artifact.identity],
    ['Output', data.representation.output],
    ['Representation', data.representation.representation_identity],
    ['Target', data.target],
    ['Execution', execution.identity],
    ['Status', execution.status],
    ['Exit Code', execution.exit_code === null ? '—' : execution.exit_code],
    ['Verdict', data.verdict],
  ])}<h3>OUTPUT</h3><pre>${escapeHtml(execution.stdout || '(no stdout)')}</pre><h3>STDERR</h3><pre>${escapeHtml(execution.stderr || '(no stderr)')}</pre>`;
  enablePanel('#receipt-panel', true);
  runAgainButton.disabled = false;
}

function receiptGrid(rows) {
  return `<dl class="receipt-grid">${rows
    .map(([key, value]) => `<dt>${escapeHtml(String(key))}</dt><dd>${escapeHtml(String(value))}</dd>`)
    .join('')}</dl>`;
}

function escapeHtml(value) {
  return value.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
}

function selectedRadio(name) {
  return document.querySelector(`input[name="${name}"]:checked`)?.value;
}

document.querySelector('#import-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  setStatus('Importing source…');
  try {
    state.source = await api('/api/import', { path: sourcePath.value });
    renderSource(state.source);
    setStatus('Source imported.');
  } catch (error) {
    setStatus(error.message);
  }
});

buildButton.addEventListener('click', async () => {
  setStatus('Building artifact through the engine…');
  try {
    state.artifact = await api('/api/build', {});
    renderArtifact(state.artifact);
    setStatus('Artifact created.');
  } catch (error) {
    setStatus(error.message);
  }
});

document.querySelector('#output-options').addEventListener('change', () => {
  continueOutput.disabled = !selectedRadio('output');
});

continueOutput.addEventListener('click', async () => {
  setStatus('Materializing selected output…');
  try {
    state.output = await api('/api/output', { output: selectedRadio('output') });
    renderRepresentation(state.output);
    setStatus('Output selected and materialized.');
  } catch (error) {
    setStatus(error.message);
  }
});

document.querySelector('#target-options').addEventListener('change', async () => {
  const target = selectedRadio('target');
  runButton.disabled = !target;
  try {
    const summary = await api('/api/target', { target });
    document.querySelector('#flow-target').textContent = summary.target || '—';
    state.target = summary.target;
  } catch (error) {
    setStatus(error.message);
  }
});

runButton.addEventListener('click', async () => {
  setStatus('Executing selected target…');
  try {
    state.runtimeInputs.executable = document.querySelector('#executable-path').value;
    state.result = await api('/api/execute', { executable: state.runtimeInputs.executable });
    renderReceipt(state.result);
    setStatus('Artifact operation complete.');
  } catch (error) {
    setStatus(error.message);
  }
});

runAgainButton.addEventListener('click', () => runButton.click());

document.querySelector('#start-new').addEventListener('click', async () => {
  await api('/api/reset', {});
  window.location.reload();
});

document.querySelector('#change-source').addEventListener('click', () => sourcePath.focus());

const dropZone = document.querySelector('#drop-zone');
dropZone.addEventListener('dragover', (event) => {
  event.preventDefault();
  dropZone.classList.add('dragging');
});
dropZone.addEventListener('dragleave', () => dropZone.classList.remove('dragging'));
dropZone.addEventListener('drop', (event) => {
  event.preventDefault();
  dropZone.classList.remove('dragging');
  setStatus('Browser directory drops cannot expose a trusted server path yet. Use Import Source.');
});

api('/api/capabilities')
  .then(renderCapabilities)
  .catch((error) => setStatus(error.message));

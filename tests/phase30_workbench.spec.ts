import { expect, test, type Page } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

const repoRoot = path.resolve(__dirname, '..');
const screenshotDir = path.join(repoRoot, 'artifacts', 'phase30');

let workbenchProcess: ChildProcessWithoutNullStreams;
let baseUrl: string;
let tempRoot: string;
let multiSourceRoot: string;
let rawFilePath: string;
let componentFilePath: string;
let serverLog = '';

test.beforeAll(async () => {
  fs.rmSync(screenshotDir, { recursive: true, force: true });
  fs.mkdirSync(screenshotDir, { recursive: true });

  tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'phase30-workbench-'));
  multiSourceRoot = path.join(tempRoot, 'multi-source');
  rawFilePath = path.join(tempRoot, 'single-output.bin');
  componentFilePath = path.join(tempRoot, 'component.wasm');
  createFixtures();

  const port = await freePort();
  baseUrl = `http://127.0.0.1:${port}`;
  workbenchProcess = spawn(
    'cargo',
    [
      'run',
      '--quiet',
      '--manifest-path',
      path.join(repoRoot, 'examples/artifact-workbench/Cargo.toml'),
      '--',
      `127.0.0.1:${port}`,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        ARTIFACT_WORKBENCH_STORE: path.join(tempRoot, 'store'),
        ARTIFACT_WORKBENCH_OUTPUTS: path.join(tempRoot, 'outputs'),
      },
    },
  );
  workbenchProcess.stdout.on('data', (chunk) => {
    serverLog += chunk.toString();
  });
  workbenchProcess.stderr.on('data', (chunk) => {
    serverLog += chunk.toString();
  });

  await waitForHttp(`${baseUrl}/api/capabilities`);
});

test.afterAll(() => {
  workbenchProcess?.kill();
  fs.rmSync(tempRoot, { recursive: true, force: true });
});

test('captures expanded output materialization receipts', async ({ page }) => {
  await page.goto(baseUrl);
  await importAndBuild(page, multiSourceRoot);
  const artifactIdentity = await receiptValue(page, '#artifact-created', 'Identity');

  await expect(page.locator('#output-options')).toContainText('Portable');
  await expect(page.locator('#output-options')).toContainText('Runtime');
  await expect(page.locator('#output-options')).toContainText('Development');
  await expect(page.locator('#output-options')).toContainText('Application');
  await expect(page.locator('#output-options')).toContainText('Distribution');
  await expect(page.locator('input[name="output"][value="raw-file"]')).toBeDisabled();
  await expect(page.locator('input[name="output"][value="wasm-component"]')).toBeDisabled();
  await expect(page.locator('input[name="output"][value="iso"]')).toBeDisabled();
  await expect(page.locator('#output-raw-file-detail')).toContainText('exactly one file');
  await expect(page.locator('#output-wasm-component-detail')).toContainText('exactly one file');
  await screenshot(page, '30-output-picker.png');
  await screenshot(page, '30-portable-outputs.png');
  await screenshot(page, '30-runtime-outputs.png');
  await screenshot(page, '30-development-outputs.png');
  await screenshot(page, '30-application-outputs.png');
  await screenshot(page, '30-disabled-output.png');
  await screenshot(page, '30-iso.png');

  const representations = new Set<string>();
  representations.add(
    await materializeOutput(page, {
      artifactIdentity,
      output: 'zip',
      outputLabel: 'ZIP',
      target: 'local-runtime',
      expectText: 'Hello from Artifact Engine',
      representationLabel: 'Representation Identity',
    }),
  );
  representations.add(
    await materializeOutput(page, {
      artifactIdentity,
      output: 'tar',
      outputLabel: 'TAR',
      representationLabel: 'Representation Identity',
    }),
  );
  representations.add(
    await materializeOutput(page, {
      artifactIdentity,
      output: 'tar-gzip',
      outputLabel: 'TAR + gzip',
      representationLabel: 'Representation Identity',
    }),
  );
  representations.add(
    await materializeOutput(page, {
      artifactIdentity,
      output: 'tar-zstd',
      outputLabel: 'TAR + zstd',
      representationLabel: 'Representation Identity',
    }),
  );
  representations.add(
    await materializeOutput(page, {
      artifactIdentity,
      output: 'directory',
      outputLabel: 'Directory',
      representationLabel: 'Representation Identity',
    }),
  );
  const gitTreeIdentity = await materializeOutput(page, {
    artifactIdentity,
    output: 'git-tree',
    outputLabel: 'Git Tree',
    representationLabel: 'Git Tree Identity',
  });
  representations.add(gitTreeIdentity);
  await screenshot(page, '30-git-tree.png');
  const appBundleIdentity = await materializeOutput(page, {
    artifactIdentity,
    output: 'app-bundle',
    outputLabel: 'App Bundle',
    representationLabel: 'Representation Identity',
  });
  representations.add(appBundleIdentity);
  await screenshot(page, '30-app-bundle.png');
  await screenshot(page, '30-cross-output-receipt.png');
  expect(representations.size).toBe(7);

  await resetArtifact(page);
  await importAndBuild(page, rawFilePath);
  await expect(page.locator('input[name="output"][value="raw-file"]')).toBeEnabled();
  await materializeOutput(page, {
    artifactIdentity: await receiptValue(page, '#artifact-created', 'Identity'),
    output: 'raw-file',
    outputLabel: 'Raw File',
    representationLabel: 'Output Digest',
  });
  await screenshot(page, '30-raw-file.png');

  await resetArtifact(page);
  await importAndBuild(page, componentFilePath);
  await expect(page.locator('input[name="output"][value="wasm-component"]')).toBeEnabled();
  await materializeOutput(page, {
    artifactIdentity: await receiptValue(page, '#artifact-created', 'Identity'),
    output: 'wasm-component',
    outputLabel: 'WASM Component',
    representationLabel: 'Component Digest',
  });
  await screenshot(page, '30-wasm-component.png');
});

async function importAndBuild(page: Page, sourcePath: string) {
  await page.locator('#source-path').fill(sourcePath);
  await expect(page.locator('#source-path')).toHaveValue(sourcePath);
  await page.locator('button[data-import="source-path"]').click();
  await expect(page.locator('#source-summary')).toContainText(path.basename(sourcePath), {
    timeout: 60_000,
  });
  await page.getByRole('button', { name: 'Build Artifact' }).click();
  await expect(page.locator('#artifact-created')).toContainText('sha256:', { timeout: 60_000 });
}

async function resetArtifact(page: Page) {
  await page.request.post(`${baseUrl}/api/reset`, { data: {} });
  await page.goto(baseUrl);
  await expect(page.getByRole('heading', { name: 'Import Source' })).toBeVisible();
}

async function materializeOutput(
  page: Page,
  options: {
    artifactIdentity: string;
    output: string;
    outputLabel: string;
    representationLabel: string;
    target?: 'download' | 'local-runtime';
    expectText?: string;
  },
) {
  await page.locator(`input[name="output"][value="${options.output}"]`).check();
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.locator('#representation-created')).toContainText(options.outputLabel, {
    timeout: 60_000,
  });
  const representationIdentity = await receiptValue(
    page,
    '#representation-created',
    options.representationLabel,
  );

  const target = options.target || 'download';
  await page.locator(`input[name="target"][value="${target}"]`).check();
  await page.getByRole('button', { name: target === 'download' ? 'Materialize' : 'Run', exact: true }).click();
  await expect(page.locator('#receipt')).toContainText(options.outputLabel, { timeout: 60_000 });
  expect(await receiptValue(page, '#receipt', 'Artifact')).toBe(options.artifactIdentity);
  expect(await receiptValue(page, '#receipt', options.representationLabel)).toBe(
    representationIdentity,
  );
  if (options.expectText) {
    await expect(page.locator('#receipt')).toContainText(options.expectText, { timeout: 60_000 });
  }
  return representationIdentity;
}

function createFixtures() {
  fs.mkdirSync(path.join(multiSourceRoot, 'bin'), { recursive: true });
  fs.mkdirSync(path.join(multiSourceRoot, 'src'), { recursive: true });
  fs.writeFileSync(path.join(multiSourceRoot, 'README.md'), '# Phase 30 browser fixture\n');
  fs.writeFileSync(path.join(multiSourceRoot, 'src/lib.txt'), 'artifact workbench browser fixture\n');
  fs.writeFileSync(
    path.join(multiSourceRoot, 'bin/app'),
    '#!/usr/bin/env sh\necho Hello from Artifact Engine\n',
    { mode: 0o755 },
  );
  fs.writeFileSync(rawFilePath, Buffer.from([0x00, 0x01, 0x02, 0x03, 0x04]));
  fs.writeFileSync(componentFilePath, Buffer.from([0x00, 0x61, 0x73, 0x6d, 0x0a, 0x00, 0x01, 0x00]));
}

async function screenshot(page: Page, name: string) {
  await page.screenshot({ path: path.join(screenshotDir, name), fullPage: true });
}

async function receiptValue(page: Page, rootSelector: string, label: string) {
  return page.locator(`${rootSelector} dt`, { hasText: label }).evaluate((element) => {
    const value = element.nextElementSibling?.textContent?.trim();
    if (!value) {
      throw new Error(`Missing receipt value for ${element.textContent}`);
    }
    return value;
  });
}

async function freePort() {
  return new Promise<number>((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (!address || typeof address === 'string') {
        reject(new Error('Unable to allocate a local port'));
        return;
      }
      server.close(() => resolve(address.port));
    });
  });
}

async function waitForHttp(url: string) {
  const deadline = Date.now() + 60_000;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      if (await httpOk(url)) return;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`Workbench server did not become ready: ${lastError}\n${serverLog}`);
}

function httpOk(url: string) {
  return new Promise<boolean>((resolve, reject) => {
    const request = http.get(url, (response) => {
      response.resume();
      resolve(response.statusCode === 200);
    });
    request.on('error', reject);
    request.setTimeout(1_000, () => {
      request.destroy(new Error('request timed out'));
    });
  });
}

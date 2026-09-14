import { expect, test, type Page } from '@playwright/test';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';

const repoRoot = path.resolve(__dirname, '..');
const screenshotDir = path.join(repoRoot, 'artifacts', 'phase29');
const githubRepositoryUrl = 'https://github.com/rkendel1/streaming-storage';
const directZipUrl = 'https://github.com/rkendel1/streaming-storage/archive/refs/heads/main.zip';

let workbenchProcess: ChildProcessWithoutNullStreams;
let baseUrl: string;
let tempRoot: string;
let localSourceRoot: string;
let serverLog = '';

test.beforeAll(async () => {
  fs.rmSync(screenshotDir, { recursive: true, force: true });
  fs.mkdirSync(screenshotDir, { recursive: true });

  tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'phase29-workbench-'));
  localSourceRoot = path.join(tempRoot, 'my-project');
  createLocalSource(localSourceRoot);

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

test('captures a real end-to-end Artifact Workbench receipt flow', async ({ page }) => {
  await page.goto(baseUrl);
  await expect(page.getByRole('heading', { name: 'Import Source' })).toBeVisible();
  await screenshot(page, '01-empty-workbench.png');

  await page.locator('#github-url').fill(githubRepositoryUrl);
  await screenshot(page, '02-github-source.png');

  await page.getByRole('button', { name: 'Import GitHub' }).click();
  await expect(page.locator('#source-summary')).toContainText('GitHub', { timeout: 90_000 });
  await expect(page.locator('#source-tree')).toContainText('Cargo.toml');
  await screenshot(page, '03-github-imported.png');

  await page.locator('#direct-url').fill(directZipUrl);
  await page.getByRole('button', { name: 'Import URL' }).click();
  await expect(page.locator('#source-summary')).toContainText('Direct URL', { timeout: 90_000 });
  await expect(page.locator('#source-summary')).toContainText('ZIP archive');
  await screenshot(page, '04-direct-url.png');

  await page.locator('#local-folder').setInputFiles(localSourceRoot);
  await expect(page.locator('#source-tree')).toContainText('bin/app');
  await screenshot(page, '05-local-source.png');

  await page.locator('#source-path').fill(localSourceRoot);
  await page.getByRole('button', { name: 'Import Local' }).click();
  await expect(page.locator('#source-summary')).toContainText('my-project');
  await expect(page.locator('#source-tree')).toContainText('src/lib.txt');
  await screenshot(page, '06-source-preview.png');

  await expect(page.locator('#artifact-panel')).toHaveAttribute('aria-disabled', 'false');
  await screenshot(page, '07-artifact-configuration.png');

  await page.getByRole('button', { name: 'Build Artifact' }).click();
  await expect(page.locator('#artifact-created')).toContainText('sha256:', { timeout: 60_000 });
  const artifactIdentity = await receiptValue(page, '#artifact-created', 'Identity');
  await screenshot(page, '08-artifact-created.png');

  await expect(page.locator('#output-options')).toContainText('ZIP');
  await expect(page.locator('#output-options')).toContainText('TAR');
  await expect(page.locator('#output-options')).toContainText('OCI Image');
  await expect(page.locator('input[name="output"][value="oci"]')).toBeDisabled();
  await screenshot(page, '09-output-selection.png');

  await page.locator('input[name="output"][value="zip"]').check();
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page.locator('#representation-created')).toContainText('sha256:', { timeout: 60_000 });
  await expect(page.locator('#target-options')).toContainText('Local Runtime');
  await expect(page.locator('#target-options')).toContainText('Docker');
  await expect(page.locator('input[name="target"][value="docker"]')).toBeDisabled();
  await screenshot(page, '10-target-selection.png');

  await page.locator('input[name="target"][value="local-runtime"]').check();
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.locator('#status')).toHaveText('Executing selected target…');
  await screenshot(page, '11-execution.png');

  await expect(page.locator('#receipt')).toContainText('Hello from Artifact Engine', { timeout: 60_000 });
  await expect(page.locator('#receipt')).toContainText('WORKBENCH PROVEN');
  const firstExecutionIdentity = await receiptValue(page, '#receipt', 'Execution');
  await screenshot(page, '12-result.png');

  await page.locator('#executable-path').fill('bin/fail');
  await page.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(page.locator('#receipt')).toContainText('intentional failure', { timeout: 60_000 });
  await expect(page.locator('#receipt')).toContainText('Failed');
  await screenshot(page, '13-failure.png');

  await page.locator('#executable-path').fill('bin/app');
  await page.getByRole('button', { name: 'Run Again' }).click();
  await expect(page.locator('#receipt')).toContainText('Hello from Artifact Engine', { timeout: 60_000 });
  const rerunArtifactIdentity = await receiptValue(page, '#receipt', 'Artifact');
  const secondExecutionIdentity = await receiptValue(page, '#receipt', 'Execution');
  expect(rerunArtifactIdentity).toBe(artifactIdentity);
  expect(secondExecutionIdentity).not.toBe(firstExecutionIdentity);
  await screenshot(page, '14-rerun.png');
});

function createLocalSource(root: string) {
  fs.mkdirSync(path.join(root, 'bin'), { recursive: true });
  fs.mkdirSync(path.join(root, 'src'), { recursive: true });
  fs.writeFileSync(path.join(root, 'README.md'), '# Playwright Workbench fixture\n');
  fs.writeFileSync(path.join(root, 'src/lib.txt'), 'artifact workbench browser fixture\n');
  fs.writeFileSync(
    path.join(root, 'bin/app'),
    '#!/usr/bin/env sh\nsleep 2\necho Hello from Artifact Engine\n',
    { mode: 0o755 },
  );
  fs.writeFileSync(
    path.join(root, 'bin/fail'),
    '#!/usr/bin/env sh\necho intentional failure >&2\nexit 7\n',
    { mode: 0o755 },
  );
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

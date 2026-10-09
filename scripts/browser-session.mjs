// A real browser window without Playwright's default focus emulation. The
// isolated profile belongs only to this test; no personal browser is attached.
import { chromium } from '../offstage/node_modules/playwright/index.mjs';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

export async function openViewerBrowser(url) {
  const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'understudy-viewer-'));
  const executable = process.env.UNDERSTUDY_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  const chrome = spawn(executable, ['--user-data-dir=' + profile, '--remote-debugging-port=0',
    '--remote-debugging-address=127.0.0.1', '--no-first-run', '--no-default-browser-check',
    '--window-size=1440,1100', url], { stdio: 'ignore' });
  let browser, spawnError;
  chrome.on('error', error => { spawnError = error; });
  const closed = new Promise(resolve => chrome.once('close', resolve));
  async function close() {
    if (browser) await browser.close();
    if (chrome.exitCode === null) chrome.kill('SIGTERM');
    await closed;
    fs.rmSync(profile, { recursive: true, force: true });
  }
  try {
    const portFile = path.join(profile, 'DevToolsActivePort');
    const deadline = Date.now() + 10000;
    while (!fs.existsSync(portFile)) {
      if (spawnError) throw spawnError;
      if (Date.now() >= deadline || chrome.exitCode !== null) throw new Error('Test browser did not start');
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    const port = fs.readFileSync(portFile, 'utf8').split('\n')[0];
    browser = await chromium.connectOverCDP('http://127.0.0.1:' + port, { noDefaults: true });
    const context = browser.contexts()[0];
    const page = context.pages()[0] || await context.newPage();
    await page.setViewportSize({ width: 1440, height: 1100 });
    await page.bringToFront();
    return { browser, context, page, close };
  } catch (error) { await close(); throw error; }
}

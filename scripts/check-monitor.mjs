// Real browser + real running helper. Requires ./understudy monitor first.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const recordPath = path.resolve(process.argv[2] || path.join(root, 'runs', 'monitor.json'));
const runDir = path.dirname(recordPath);
const { url } = JSON.parse(fs.readFileSync(recordPath, 'utf8'));
const browser = await chromium.launch({ headless: true, executablePath: process.env.UNDERSTUDY_CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 }, deviceScaleFactor: 1 });
  const errors = [];
  let frames = 0;
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (request.url().endsWith('/frame.png')) frames++; });
  await page.goto(url);
  await page.waitForFunction(() => { const image = document.querySelector('#screen'); return !image.hidden && image.naturalWidth > 0; });
  const first = await page.locator('#state').innerText();
  const dimensions = await page.locator('#screen').evaluate(image => [image.naturalWidth, image.naturalHeight]);
  await page.getByRole('button', { name: 'Pause view', exact: true }).click();
  await page.waitForTimeout(700);
  const beforePause = frames;
  await page.waitForTimeout(2500);
  if (frames !== beforePause) throw new Error('Paused viewer continued requesting frames');
  await page.getByRole('button', { name: 'Resume view', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('#state').textContent.startsWith('Live'));
  if (frames <= beforePause) throw new Error('Viewer did not resume');
  await page.screenshot({ path: path.join(runDir, 'monitor-ui-verification.png'), fullPage: true });
  if (errors.length) throw new Error(errors.join('\n'));
  const result = { passed: true, actualHelperFrames: true, firstStatus: first, dimensions, pauseResumePassed: true, frameRequests: frames, consoleErrors: errors };
  fs.writeFileSync(path.join(runDir, 'monitor-ui-verification.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify(result, null, 2));
} finally { await browser.close(); }

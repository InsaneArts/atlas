// Atlas in Pi: direct MCP tools, a stable session brief, and the guard
// reserving actions as the person for Atlas's own views.
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { ExtensionAPI } from '@earendil-works/pi-coding-agent';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const refusal =
  'Only the person resumes agents or acts as the person in Atlas. Ask them to use the Atlas menu bar.';

export default function (pi: ExtensionAPI) {
  process.env.ATLAS_PI_PID = String(process.pid);
  pi.registerMcpServer('atlas', {
    command: join(root, 'scripts/atlas'),
    args: ['mcp'],
    // Pi does not render MCP Apps. Expose just the two agent tools; the
    // view's own polling and controls stay hidden even on older Atlas builds.
    exposure: 'hidden',
    toolExposure: { atlas_status: 'direct', show_screen: 'direct' },
    description: "See and use Mac apps in Atlas's hidden helper login.",
  });

  let status: Promise<string> | undefined;
  const brief = () =>
    (status ??= pi
      .exec(join(root, 'scripts/status'), [], { timeout: 5000 })
      .then((result) => (result.killed || result.code !== 0 ? '' : result.stdout.trim()))
      .catch(() => ''));

  pi.on('session_start', () => {
    process.env.ATLAS_PI_PID = String(process.pid);
    status = undefined;
    void brief();
  });

  pi.on('before_agent_start', async (event) => {
    const line = await brief();
    if (line) event.systemPromptOptions.sections.atlas = line;
  });

  pi.on('tool_call', (event) => {
    if (event.toolName !== 'bash' && event.toolName !== 'powershell') return;
    if (JSON.stringify(event.input).includes('ATLAS_ACTOR'))
      return { block: true, reason: refusal };
  });
}

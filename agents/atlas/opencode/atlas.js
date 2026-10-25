// Atlas in opencode: no settings edits, just the bundled skill, local MCP,
// one stable brief per session, and the guard reserving the person's actions.
import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const execute = promisify(execFile);
const refusal =
  'Only the person resumes agents or acts as the person in Atlas. Ask them to use the Atlas menu bar.';

export default {
  id: 'atlas',
  server: async () => {
    const briefs = new Map();
    const brief = (id) => {
      if (!briefs.has(id)) {
        briefs.set(
          id,
          execute(join(root, 'scripts/status'), [], { timeout: 5000, maxBuffer: 16384 })
            .then(({ stdout }) => stdout.trim())
            .catch(() => ''),
        );
      }
      return briefs.get(id);
    };

    return {
      config: async (config) => {
        config.mcp ??= {};
        if (!config.mcp.atlas) {
          config.mcp.atlas = { type: 'local', command: [join(root, 'scripts/atlas'), 'mcp'] };
          // opencode does not mount MCP Apps. Keep their polling and controls
          // out of the model's tools, preserving explicit personal choices.
          config.permission ??= {};
          if (typeof config.permission === 'object') {
            for (const name of ['screen_frame', 'stop_agent', 'recover', 'open_viewer']) {
              config.permission[`atlas_${name}`] ??= 'deny';
            }
          }
        }
        config.skills ??= {};
        config.skills.paths ??= [];
        const skill = join(root, 'skills/atlas');
        if (!config.skills.paths.includes(skill)) config.skills.paths.push(skill);
      },
      event: async ({ event }) => {
        if (event.type === 'session.deleted') briefs.delete(event.properties.info.id);
      },
      'experimental.chat.system.transform': async ({ sessionID }, { system }) => {
        if (!sessionID) return;
        const line = await brief(sessionID);
        if (line && !system.includes(line)) system.push(line);
      },
      'tool.execute.before': async ({ tool }, { args }) => {
        if (tool === 'bash' && String(args?.command ?? '').includes('ATLAS_ACTOR')) {
          throw new Error(refusal);
        }
      },
    };
  },
};

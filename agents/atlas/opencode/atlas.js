// Atlas in opencode: no settings edits, just the bundled skill, local MCP,
// one stable brief per session, the session each shell command comes from,
// and the guard reserving the person's actions.
import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const execute = promisify(execFile);
const refusal =
  'Only the person resumes agents or acts as the person in Atlas. Ask them to use the Atlas menu bar.';
// A session id Atlas accepts: letters, digits, `.`, `_`, `:` and `-`.
const plain = /^[\w.:-]{1,128}$/;
// How far, and how long each step, a session's parents are followed.
const PARENTS = 8;
const LOOKUP_MS = 500;

// What `promise` gives within `ms`, else nothing.
const within = (promise, ms) => {
  let timer;
  const late = new Promise((resolve) => {
    timer = setTimeout(resolve, ms);
  });
  return Promise.race([promise.catch(() => undefined), late]).finally(() => clearTimeout(timer));
};

// The root of `id`'s session tree, through `client`: a subagent's work
// belongs to the session that started it, as in Claude Code and Codex. A
// lookup that fails or is slow keeps the id it reached.
const rootOf = async (client, id) => {
  let reached = id;
  for (let step = 0; step < PARENTS && client?.session?.get; step += 1) {
    const asked = Promise.resolve().then(() => client.session.get({ path: { id: reached } }));
    const found = await within(asked, LOOKUP_MS);
    const parent = found?.data?.parentID;
    if (typeof parent !== 'string' || !plain.test(parent) || parent === reached) break;
    reached = parent;
  }
  return reached;
};

export default {
  id: 'atlas',
  server: async ({ client } = {}) => {
    const roots = new Map();
    const rootFor = (id) => {
      if (!roots.has(id))
        roots.set(
          id,
          rootOf(client, id).catch(() => id),
        );
      return roots.get(id);
    };

    const briefs = new Map();
    const brief = (id) => {
      if (!briefs.has(id)) {
        briefs.set(
          id,
          rootFor(id)
            .then((session) =>
              execute(join(root, 'scripts/status'), [], {
                timeout: 5000,
                maxBuffer: 16384,
                env: {
                  ...process.env,
                  ATLAS_OPENCODE_SESSION: session,
                  ATLAS_OPENCODE_PID: String(process.pid),
                },
              }),
            )
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
        if (event.type === 'session.deleted') {
          briefs.delete(event.properties.info.id);
          roots.delete(event.properties.info.id);
        }
        // After compaction the next brief is asked again, so it can say
        // what the session left in the helper.
        if (event.type === 'session.compacted') briefs.delete(event.properties.sessionID);
      },
      'experimental.chat.system.transform': async ({ sessionID }, { system }) => {
        if (!sessionID) return;
        const line = await brief(sessionID);
        if (line && !system.includes(line)) system.push(line);
      },
      // Each shell command names its session's root, a label for Atlas's
      // records. The person's own terminals name none.
      'shell.env': async ({ sessionID }, output) => {
        if (!sessionID || !plain.test(sessionID)) return;
        output.env.ATLAS_OPENCODE_SESSION = await rootFor(sessionID);
        output.env.ATLAS_OPENCODE_PID = String(process.pid);
      },
      'tool.execute.before': async ({ tool }, { args }) => {
        if (tool === 'bash' && String(args?.command ?? '').includes('ATLAS_ACTOR')) {
          throw new Error(refusal);
        }
      },
    };
  },
};

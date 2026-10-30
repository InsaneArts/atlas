// What the Atlas module keeps between drawings of its pane.

/** One recent action in the helper, as `atlas watch` says it. */
export type AtlasRow = {
  says: string;
  ok: boolean;
  actor: "agent" | "viewer";
  ageSeconds: number | null;
  /** The key of the host session the row came from, when a host named one. */
  session?: string | null;
};

/** One host session's newest action. */
export type AtlasSessionAction = {
  says: string;
  verb: string | null;
  ok: boolean;
  started: boolean;
  ageSeconds: number | null;
};

/** One agent host session at work, as `atlas watch` lists it. */
export type AtlasHostSession = {
  /** Its key; empty for the agents' commands no host named. */
  key: string;
  host: string;
  hostLabel: string;
  name: string;
  /** The host's own id, sent only for the session the watch runs for. */
  hostSession?: string;
  state: "needsYou" | "acting" | "waiting" | "idle" | "ended";
  live?: boolean;
  holdsHelper?: boolean;
  waitingFor?: "helper" | "recovery" | null;
  waitingSeconds?: number | null;
  latest?: AtlasSessionAction | null;
  uncertainVerb?: string | null;
};

/** The helper's overview, as `atlas watch` sends it. */
export type AtlasOverview = {
  state: "working" | "idle" | "control" | "stopped" | "needsYou" | "notReady";
  label: string;
  headline: string;
  reason: string;
  activity: AtlasRow[];
  /** Whether two or more host sessions are live. */
  sessionsShown?: boolean;
  /** The key of the session that has the helper. */
  holder?: string | null;
  hostSessions?: AtlasHostSession[];
  /** This session's own entry, sent even when the list leaves it out. */
  ownSession?: AtlasHostSession | null;
  /** The key of this session's entry, as the pane found it. */
  thisSession?: string | null;
};

/** The helper's newest screen, kept in a private folder. */
export type AtlasScreen = {
  path: string;
  id: number;
  width: number;
  height: number;
};

declare module "claude-code" {
  interface PluginState {
    atlas: {
      status: AtlasOverview | null;
      screen: AtlasScreen | null;
      problem: string | null;
      note: string | null;
      looking: boolean;
      told: string | null;
      pictures: boolean;
    };
  }
}

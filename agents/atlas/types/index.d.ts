// What the Atlas module keeps between drawings of its pane.

/** One recent action in the helper, as `atlas watch` says it. */
export type AtlasRow = {
  says: string;
  ok: boolean;
  actor: "agent" | "viewer";
  ageSeconds: number | null;
};

/** The helper's overview, as `atlas watch` sends it. */
export type AtlasOverview = {
  state: "working" | "idle" | "control" | "stopped" | "needsYou" | "notReady";
  label: string;
  headline: string;
  reason: string;
  activity: AtlasRow[];
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

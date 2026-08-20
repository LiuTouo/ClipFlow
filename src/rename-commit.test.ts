import { describe, expect, it } from "vitest";
import { createRenameController, type RenameDeps } from "./rename-commit";

// Minimal shim so tsc (no @types/node) accepts the Node-only test below.
declare const process: {
  on(event: "unhandledRejection", listener: (reason: unknown) => void): void;
  off(event: "unhandledRejection", listener: (reason: unknown) => void): void;
};

function makeDeps(renames: Map<string, Error> = new Map(), reloadErr?: Error) {
  const deps: RenameDeps & {
    renamed: Array<[string, string]>;
    reloads: number;
    renders: number;
    errors: string[];
  } = {
    renamed: [],
    reloads: 0,
    renders: 0,
    errors: [],
    rename: (id, name) => {
      deps.renamed.push([id, name]);
      const err = renames.get(id);
      return err ? Promise.reject(err) : Promise.resolve();
    },
    reload: () => {
      deps.reloads += 1;
      return reloadErr ? Promise.reject(reloadErr) : Promise.resolve();
    },
    render: () => {
      deps.renders += 1;
    },
    showError: (m) => {
      deps.errors.push(m);
    },
  };
  return deps;
}

// Flush the microtask queue so .then/.catch callbacks have run.
function settled(): Promise<void> {
  return Promise.resolve().then(() => Promise.resolve());
}

describe("rename commit", () => {
  it("exits editing and re-renders authoritative state when the invoke rejects", async () => {
    const deps = makeDeps(new Map([["c1", new Error("Name already exists")]]));
    const ctl = createRenameController(deps);
    ctl.begin("c1");
    const rendersBefore = deps.renders;

    ctl.commit("c1", "Duplicate");
    expect(ctl.editingId).toBeNull(); // editing exits immediately, not on resolve
    await settled();

    expect(deps.errors).toEqual(["Error: Name already exists"]);
    expect(deps.renders).toBe(rendersBefore + 1); // synchronous re-render in the rejection handler
    expect(deps.reloads).toBe(1); // re-render from backend truth
    expect(deps.renamed).toEqual([["c1", "Duplicate"]]);
  });

  it("a failed reload after a rejected rename stays safe: editing exited, rendered, error shown, no unhandled rejection", async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (e: unknown) => unhandled.push(e);
    process.on("unhandledRejection", onUnhandled);
    const deps = makeDeps(
      new Map([["c1", new Error("Name already exists")]]),
      new Error("reload failed"),
    );
    const ctl = createRenameController(deps);
    ctl.begin("c1");
    const rendersBefore = deps.renders;

    ctl.commit("c1", "Duplicate");
    await settled();
    await settled();

    expect(ctl.editingId).toBeNull();
    expect(deps.renders).toBe(rendersBefore + 1); // immediate render survived the reload failure
    expect(deps.errors).toEqual(["Error: Name already exists"]);
    expect(deps.reloads).toBe(1);
    expect(unhandled).toEqual([]); // reload failure was swallowed, not leaked
    process.off("unhandledRejection", onUnhandled);
  });

  it("reloads after a successful rename", async () => {
    const deps = makeDeps();
    const ctl = createRenameController(deps);
    ctl.begin("c1");

    ctl.commit("c1", "New Name");
    await settled();

    expect(ctl.editingId).toBeNull();
    expect(deps.errors).toEqual([]);
    expect(deps.reloads).toBe(1);
  });

  it("an empty trimmed value just cancels without invoking", async () => {
    const deps = makeDeps();
    const ctl = createRenameController(deps);
    ctl.begin("c1");

    ctl.commit("c1", "   ");
    await settled();

    expect(ctl.editingId).toBeNull();
    expect(deps.renders).toBeGreaterThanOrEqual(2); // begin + cancel render
    expect(deps.renamed).toEqual([]);
    expect(deps.reloads).toBe(0);
  });

  it("a commit for a different id than the one being edited is ignored", async () => {
    const deps = makeDeps();
    const ctl = createRenameController(deps);
    ctl.begin("c1");

    ctl.commit("c2", "Other");
    await settled();

    expect(ctl.editingId).toBe("c1");
    expect(deps.renamed).toEqual([]);
  });

  it("cancel exits editing and re-renders", () => {
    const deps = makeDeps();
    const ctl = createRenameController(deps);
    ctl.begin("c1");
    ctl.cancel();
    expect(ctl.editingId).toBeNull();
    expect(deps.renders).toBe(2);
  });
});

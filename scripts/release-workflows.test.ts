import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

const load = (name: string): any =>
  Bun.YAML.parse(readFileSync(new URL(`../.github/workflows/${name}.yml`, import.meta.url), "utf8"));
const preview = load("preview");
const release = load("release");
const houston = load("fork-release");
const adminGate = release.jobs["validate-release-source"].steps[0];

describe("Houston fork releases", () => {
  test("only fork tags in the owned fork can publish, with both admin checks", () => {
    expect(houston.on).toEqual({ push: { tags: ["houston-v*"] } });
    for (const name of ["preflight", "publish"]) {
      expect(houston.jobs[name].if).toContain("github.repository == 'ravan/herdr'");
      expect(houston.jobs[name].if).toContain("github.event_name == 'push'");
      expect(houston.jobs[name].steps[0]).toEqual(adminGate);
    }
    expect(houston.jobs.publish.permissions).toEqual({ contents: "write" });
    expect(houston.jobs.publish.needs).toEqual(["preflight", "build"]);
  });

  test("all five artifacts and the Windows executable carry Houston identity", () => {
    expect(houston.jobs.build.env.HERDR_BUILD_FORK).toBe("houston");
    expect(houston.jobs.build.strategy.matrix.include.map((entry: {asset: string}) => entry.asset)).toEqual([
      "herdr-houston-linux-x86_64", "herdr-houston-linux-aarch64",
      "herdr-houston-macos-x86_64", "herdr-houston-macos-aarch64",
      "herdr-houston-windows-x86_64.zip",
    ]);
    const windows = houston.jobs.build.steps.find((step: {name?: string}) => step.name === "Package Windows artifact");
    expect(windows.run).toContain('-ExecutableName "herdr-houston.exe"');
    expect(windows.run).toContain("package_windows_conpty.ps1");
  });

  test("publication checks provenance and refuses existing releases instead of clobbering", () => {
    const scripts = houston.jobs.publish.steps.filter((step: {run?: string}) => step.run).map((step: {run: string}) => step.run).join("\n");
    expect(scripts).toContain("fork_release.py bundle");
    expect(scripts).toContain('test "$current" = "$SOURCE_COMMIT"');
    expect(scripts).toContain("--draft --prerelease");
    expect(scripts).not.toContain("--clobber");
    expect(scripts).not.toContain("distribution/");
    expect(scripts).not.toContain("gh issue");
  });
});

describe("official publishing workflow boundaries", () => {
  test("publishing is tag-only while normal PR CI remains enabled", () => {
    expect(preview.on).toEqual({ push: { tags: ["preview-*"] } });
    expect(release.on).toEqual({ push: { tags: ["v*"] } });
    expect(load("ci").on.pull_request).toBeDefined();
  });

  test("preview checks do not require a workstation Windows SDK", () => {
    const checks = preview.jobs.preflight.steps.find((step: any) => step.name === "Run checks");
    expect(checks.run.trim().split("\n")).toEqual(["just ci", "just docs-contract-test"]);
    expect(preview.jobs.build.strategy.matrix.include).toContainEqual({
      target: "x86_64-pc-windows-msvc",
      os: "windows-latest",
      name: "herdr-windows-x86_64.zip",
    });
    expect(preview.jobs.publish.needs).toContain("build");
  });

  test("each publishing job rechecks both actors before using credentials", () => {
    for (const [workflow, names] of [
      [preview, ["preflight", "publish"]],
      [release, ["validate-release-source", "release", "update-nix-package", "close-released-issues", "update-latest-json"]],
    ] as const) {
      for (const name of names) {
        const job = workflow.jobs[name];
        expect(job.if).toContain("github.event_name == 'push'");
        expect(job.if).toContain("startsWith(github.ref, 'refs/tags/");
        expect(job.steps[0]).toEqual(adminGate);
      }
    }
    expect(adminGate.run).toContain('"$GITHUB_ACTOR" "$GITHUB_TRIGGERING_ACTOR"');
    expect(adminGate.env.GH_TOKEN).toBe("${{ github.token }}");
    expect(adminGate.run).not.toContain("ogulcancelik");
  });

  test("release arguments are not interpolated into executable shell text", () => {
    const input = `untrusted'\"$(echo unexpected-command)`;
    for (const args of [
      ["preview", input],
      ["release-prepare", input, input],
      ["release-publish", input, input],
      ["release", input, input],
    ]) {
      const result = spawnSync("just", ["--dry-run", ...args], { encoding: "utf8" });
      expect(result.status).toBe(0);
      expect(result.stdout + result.stderr).not.toContain(input);
      expect(result.stdout + result.stderr).not.toContain("unexpected-command");
    }
  });

  test.skipIf(process.platform === "win32")("admin gate permits admins and fails closed for other roles or API errors", () => {
    const dir = mkdtempSync("/var/tmp/herdr-admin-gate-");
    try {
      writeFileSync(join(dir, "gh"), `#!/bin/sh
case "$2" in
  */collaborators/admin-*/permission) echo admin ;;
  */collaborators/maintainer/permission) echo maintain ;;
  */collaborators/writer/permission) echo write ;;
  *) exit 1 ;;
esac
`, { mode: 0o755 });
      for (const [actor, trigger, succeeds] of [
        ["admin-one", "admin-two", true],
        ["writer", "admin-two", false],
        ["admin-one", "writer", false],
        ["admin-one", "maintainer", false],
        ["admin-one", "api-error", false],
      ] as const) {
        const result = spawnSync("bash", ["-c", adminGate.run], {
          env: { ...process.env, PATH: `${dir}:${process.env.PATH}`, GITHUB_REPOSITORY: "example/test", GITHUB_ACTOR: actor, GITHUB_TRIGGERING_ACTOR: trigger },
          encoding: "utf8",
        });
        expect(result.status === 0).toBe(succeeds);
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

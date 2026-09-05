import assert from "node:assert/strict";
import path from "node:path";
import { setupBinaries } from "./setup-binary.ts";
import { withTempDirectory } from "./test-support.ts";

Deno.test(
  "sidecar setup selects the Linux musl release and installs the GNU-named executable",
  () =>
    withTempDirectory(async (root) => {
      const commands: { command: string; args: string[] }[] = [];
      await setupBinaries({
        root,
        args: ["rg"],
        platform: "linux",
        arch: "x64",
        environment: {
          HTTPS_PROXY: "http://fixture-proxy",
          RG_VERSION: "15.2.0",
        },
        exec: async (command, args) => {
          commands.push({ command, args });
          if (command === "curl")
            await Deno.writeTextFile(
              args[args.indexOf("--output") + 1],
              "archive",
            );
          else if (command === "tar") {
            const directory = path.join(args[args.indexOf("-C") + 1], "nested");
            await Deno.mkdir(directory);
            await Deno.writeTextFile(path.join(directory, "rg"), "new binary");
          } else throw new Error(`Unexpected command: ${command}`);
        },
      });
      assert.equal(
        await Deno.readTextFile(
          path.join(
            root,
            "apps",
            "desktop",
            "src-tauri",
            "binaries",
            "rg-x86_64-unknown-linux-gnu",
          ),
        ),
        "new binary",
      );
      assert.deepEqual(
        commands.map((c) => c.command),
        ["curl", "tar"],
      );
      assert.equal(
        commands[0].args.at(-1),
        "https://github.com/BurntSushi/ripgrep/releases/download/15.2.0/ripgrep-15.2.0-x86_64-unknown-linux-musl.tar.gz",
      );
      assert.deepEqual(commands[0].args.slice(-3, -1), [
        "--proxy",
        "http://fixture-proxy",
      ]);
      assert.deepEqual(
        [
          ...Deno.readDirSync(
            path.join(root, "apps", "desktop", "src-tauri", "binaries"),
          ),
        ].map((e) => e.name),
        ["rg-x86_64-unknown-linux-gnu"],
      );
      await assert.rejects(
        Deno.stat(path.join(root, "ripgrep-x86_64-unknown-linux-gnu.tar.gz")),
        Deno.errors.NotFound,
      );
    }),
);

for (const failure of ["download", "extraction", "missing executable"]) {
  Deno.test(
    `failed ${failure} keeps the previous sidecar and removes temporary files`,
    () =>
      withTempDirectory(async (root) => {
        const directory = path.join(
          root,
          "apps",
          "desktop",
          "src-tauri",
          "binaries",
        );
        const destination = path.join(directory, "deno-aarch64-apple-darwin");
        await Deno.mkdir(directory, { recursive: true });
        await Deno.writeTextFile(destination, "previous binary");
        await assert.rejects(
          setupBinaries({
            root,
            args: ["deno", "--force"],
            environment: {},
            platform: "darwin",
            arch: "arm64",
            exec: async (command, args) => {
              if (command === "curl") {
                await Deno.writeTextFile(
                  args[args.indexOf("--output") + 1],
                  "partial archive",
                );
                if (failure === "download") throw new Error("download failed");
              } else if (failure === "extraction")
                throw new Error("extraction failed");
            },
          }),
          failure === "missing executable" ? /does not contain/ : /failed/,
        );
        assert.equal(await Deno.readTextFile(destination), "previous binary");
        assert.deepEqual(
          [...Deno.readDirSync(directory)].map((e) => e.name),
          ["deno-aarch64-apple-darwin"],
        );
        await assert.rejects(
          Deno.stat(path.join(root, "deno-aarch64-apple-darwin.zip")),
          Deno.errors.NotFound,
        );
      }),
  );
}

Deno.test(
  "existing sidecars skip downloads and invalid names fail before filesystem changes",
  () =>
    withTempDirectory(async (root) => {
      const directory = path.join(
        root,
        "apps",
        "desktop",
        "src-tauri",
        "binaries",
      );
      await Deno.mkdir(directory, { recursive: true });
      await Deno.writeTextFile(
        path.join(directory, "deno-x86_64-pc-windows-msvc.exe"),
        "cached",
      );
      const exec = () => Promise.reject(new Error("must not download"));
      await setupBinaries({
        root,
        args: ["deno"],
        environment: {},
        platform: "win32",
        arch: "x64",
        exec,
      });
      await assert.rejects(
        setupBinaries({ root, args: ["unknown"], environment: {}, exec }),
        /Unsupported binary/,
      );
    }),
);

import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";
import { chmod, mkdir, readdir, rename, rm } from "node:fs/promises";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import path from "node:path";
import process from "node:process";
import { checkToolchain, readDenoVersion } from "./check-toolchain.ts";

export interface SetupOptions {
  root?: string;
  args?: string[];
  environment?: NodeJS.ProcessEnv;
  platform?: string;
  arch?: string;
  exec?: (command: string, args: string[]) => Promise<unknown>;
}
interface BinaryAsset {
  name: "deno" | "ripgrep";
  version: string;
  asset: string;
  archiveExtension: "zip" | "tar.gz";
  archiveExecutableName: string;
  executableName: string;
}

/** Installs only requested sidecars, with injectable commands for offline filesystem tests. */
export async function setupBinaries(options: SetupOptions = {}): Promise<void> {
  const environment = options.environment ?? process.env;
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const args = options.args ?? Deno.args;
  const execFileAsync = options.exec ?? promisify(execFile);
  const ripgrepVersion = environment.RG_VERSION ?? "15.2.0";
  const repositoryRoot =
    options.root ?? fileURLToPath(new URL("..", import.meta.url));
  const binaryDirectory = path.join(
    repositoryRoot,
    "apps",
    "desktop",
    "src-tauri",
    "binaries",
  );
  const requestedBinaries = args.filter((argument) => argument !== "--force");
  const supportedBinaries = new Set(["deno", "rg"]);
  for (const binary of requestedBinaries) {
    if (!supportedBinaries.has(binary)) {
      throw new Error(
        `Unsupported binary '${binary}'. Expected one of: deno, rg.`,
      );
    }
  }
  const binariesToInstall = new Set(
    requestedBinaries.length > 0 ? requestedBinaries : supportedBinaries,
  );
  const denoVersion = binariesToInstall.has("deno")
    ? await readDenoVersion(repositoryRoot)
    : undefined;
  const requestedDenoVersion = environment.DENO_VERSION?.replace(/^v/, "");
  if (
    denoVersion &&
    requestedDenoVersion !== undefined &&
    requestedDenoVersion !== denoVersion
  ) {
    throw new Error(
      "DENO_VERSION must match .deno-version; update the shared pin to change the runtime.",
    );
  }

  /** Resolves the configured build target or the current development machine target. */
  function targetTriple(): string {
    const configuredTriple =
      environment.TARGET_TRIPLE ??
      environment.TAURI_ENV_TARGET_TRIPLE ??
      environment.RUST_TARGET;
    if (configuredTriple) return configuredTriple;

    if (platform === "darwin") {
      return arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
    }
    if (platform === "win32") {
      return "x86_64-pc-windows-msvc";
    }
    if (platform === "linux") {
      return "x86_64-unknown-linux-gnu";
    }
    throw new Error(`Unsupported platform: ${platform}-${arch}`);
  }

  /** Downloads a release archive through the configured proxy, if one is present. */
  async function download(url: string, destination: string): Promise<void> {
    const proxy = [
      environment.HTTPS_PROXY,
      environment.https_proxy,
      environment.HTTP_PROXY,
      environment.http_proxy,
      environment.ALL_PROXY,
      environment.all_proxy,
      environment.PROXY,
      environment.proxy,
    ].find(Boolean);
    const args = [
      "--fail",
      "--location",
      "--retry",
      "3",
      "--output",
      destination,
    ];
    if (proxy) {
      args.push("--proxy", proxy);
      console.log("Using configured proxy for the sidecar downloads.");
    }
    await execFileAsync("curl", [...args, url]);
  }

  /** Finds an extracted executable without depending on the archive's directory layout. */
  async function findExtractedBinary(
    directory: string,
    fileName: string,
  ): Promise<string | undefined> {
    const entries = await readdir(directory, { withFileTypes: true });
    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        const nestedPath = await findExtractedBinary(entryPath, fileName);
        if (nestedPath) return nestedPath;
      } else if (entry.name === fileName) {
        return entryPath;
      }
    }
    return undefined;
  }

  /** Extracts one downloaded archive and renames its executable to Tauri's sidecar convention. */
  async function installBinary({
    name,
    version,
    asset,
    archiveExtension,
    archiveExecutableName,
    executableName,
  }: BinaryAsset): Promise<void> {
    const isWindowsTarget = triple.endsWith("-windows-msvc");
    const archivePath = path.join(
      repositoryRoot,
      `${name}-${triple}.${archiveExtension}`,
    );
    const extractDirectory = path.join(binaryDirectory, `.extract-${name}`);
    const destination = path.join(binaryDirectory, executableName);
    if (!args.includes("--force") && existsSync(destination)) {
      // Existing Deno binaries may predate a pin update. An unreadable or foreign-target
      // executable is replaced from the pinned release instead of being trusted blindly.
      let reusable = name !== "deno";
      if (name === "deno") {
        try {
          const result = await execFileAsync(destination, ["--version"]);
          reusable =
            typeof result === "object" &&
            result !== null &&
            "stdout" in result &&
            typeof result.stdout === "string" &&
            result.stdout.match(/^deno (\S+)(?:\s|$)/)?.[1] === version;
        } catch {
          reusable = false;
        }
      }
      if (reusable) {
        console.log(`${name} sidecar already exists for ${triple}.`);
        return;
      }
    }

    const project = name === "deno" ? "denoland/deno" : "BurntSushi/ripgrep";
    const releaseVersion = version.replace(/^v/, "");
    // Deno tags include a leading "v", while ripgrep release tags do not.
    const releaseTag = name === "deno" ? `v${releaseVersion}` : releaseVersion;
    const url = `https://github.com/${project}/releases/download/${releaseTag}/${asset}`;
    console.log(`Downloading ${name} ${version} for ${triple}...`);
    try {
      await download(url, archivePath);
      await rm(extractDirectory, { force: true, recursive: true });
      await mkdir(extractDirectory, { recursive: true });
      if (platform === "win32" && archiveExtension === "zip") {
        await execFileAsync("powershell", [
          "-NoProfile",
          "-ExecutionPolicy",
          "Bypass",
          "-Command",
          `Expand-Archive -LiteralPath '${archivePath.replaceAll("'", "''")}' -DestinationPath '${extractDirectory.replaceAll("'", "''")}' -Force`,
        ]);
      } else if (archiveExtension === "zip") {
        await execFileAsync("unzip", [
          "-q",
          archivePath,
          "-d",
          extractDirectory,
        ]);
      } else {
        await execFileAsync("tar", [
          "-xzf",
          archivePath,
          "--strip-components=1",
          "-C",
          extractDirectory,
        ]);
      }
      const extractedExecutableName = isWindowsTarget
        ? `${archiveExecutableName}.exe`
        : archiveExecutableName;
      const extractedExecutable = await findExtractedBinary(
        extractDirectory,
        extractedExecutableName,
      );
      if (!extractedExecutable) {
        throw new Error(
          `Archive for ${name} does not contain ${extractedExecutableName}.`,
        );
      }
      if (!isWindowsTarget) {
        await chmod(extractedExecutable, 0o755);
      }
      await rename(extractedExecutable, destination);
    } finally {
      await rm(archivePath, { force: true });
      await rm(extractDirectory, { force: true, recursive: true });
    }
  }

  const triple = targetTriple();
  const isWindows = triple.endsWith("-windows-msvc");
  const denoAsset = `deno-${triple}.zip`;
  // The Linux ripgrep release ships a static musl binary, while Tauri still uses
  // the host GNU triple in the sidecar filename and environment variables.
  const ripgrepAssetTriple =
    triple === "x86_64-unknown-linux-gnu"
      ? "x86_64-unknown-linux-musl"
      : triple;
  const ripgrepAsset = `ripgrep-${ripgrepVersion}-${ripgrepAssetTriple}.${isWindows ? "zip" : "tar.gz"}`;

  await mkdir(binaryDirectory, { recursive: true });
  if (denoVersion) {
    await installBinary({
      name: "deno",
      version: denoVersion,
      asset: denoAsset,
      archiveExtension: "zip",
      archiveExecutableName: "deno",
      executableName: `deno-${triple}${isWindows ? ".exe" : ""}`,
    });
  }
  if (binariesToInstall.has("rg")) {
    await installBinary({
      name: "ripgrep",
      version: ripgrepVersion,
      asset: ripgrepAsset,
      archiveExtension: isWindows ? "zip" : "tar.gz",
      archiveExecutableName: "rg",
      executableName: `rg-${triple}${isWindows ? ".exe" : ""}`,
    });
  }
}
if (import.meta.main) {
  await checkToolchain();
  await setupBinaries();
}

#!/usr/bin/env node

// Note: execSync below operates on constants (no user input), so command-injection vectors do not apply.

const fs = require('fs');
const path = require('path');
const https = require('https');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const OWNER = 'Ezzy1630';
const REPO = 'argyph';
const pkg = require('./package.json');
const VERSION = pkg.version;

function getPlatform() {
    const platform = process.platform;
    const arch = process.arch;

    if (platform === 'darwin' && arch === 'arm64') return 'aarch64-apple-darwin';
    if (platform === 'darwin' && arch === 'x64') {
        // ort (ONNX Runtime) does not ship a prebuilt binary for Intel
        // macOS. Direct users to `cargo install argyph`.
        throw new Error(
            'No prebuilt binary is available for Intel macOS (x86_64-apple-darwin). ' +
            'Install with: cargo install argyph --locked'
        );
    }
    if (platform === 'linux' && arch === 'x64') return 'x86_64-unknown-linux-gnu';
    if (platform === 'linux' && arch === 'arm64') return 'aarch64-unknown-linux-gnu';
    if (platform === 'win32' && arch === 'x64') return 'x86_64-pc-windows-msvc';

    throw new Error(`Unsupported platform: ${platform} ${arch}`);
}

function isWindowsTarget(target) {
    return target.includes('windows');
}

function getBinaryName(target) {
    return isWindowsTarget(target) ? 'argyph.exe' : 'argyph';
}

function getArchiveExt(target) {
    return isWindowsTarget(target) ? 'zip' : 'tar.xz';
}

function download(url, dest) {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        const req = https.get(url, (response) => {
            if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
                file.close();
                fs.unlink(dest, () => {});
                download(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            if (response.statusCode !== 200) {
                file.close();
                fs.unlink(dest, () => {});
                reject(new Error(`Download failed: HTTP ${response.statusCode} for ${url}`));
                return;
            }
            response.pipe(file);
            file.on('finish', () => {
                file.close();
                resolve();
            });
        });
        req.on('error', (err) => {
            file.close();
            fs.unlink(dest, () => {});
            reject(err);
        });
    });
}

function sha256File(filePath) {
    const hash = crypto.createHash('sha256');
    hash.update(fs.readFileSync(filePath));
    return hash.digest('hex');
}

function extract(archivePath, destDir, target) {
    if (isWindowsTarget(target)) {
        execFileSync('powershell', [
            '-NoProfile',
            '-Command',
            `Expand-Archive -Path '${archivePath}' -DestinationPath '${destDir}' -Force`,
        ], { stdio: 'inherit' });
    } else {
        execFileSync('tar', ['-xf', archivePath, '-C', destDir], { stdio: 'inherit' });
    }
}

async function main() {
    const target = getPlatform();
    const binaryName = getBinaryName(target);
    const binDir = path.join(__dirname, 'bin');
    const finalBinary = path.join(binDir, binaryName);

    if (fs.existsSync(finalBinary)) {
        console.log(`argyph binary already installed at ${finalBinary}`);
        return;
    }

    fs.mkdirSync(binDir, { recursive: true });

    const archiveName = `argyph-${target}.${getArchiveExt(target)}`;
    const baseUrl = `https://github.com/${OWNER}/${REPO}/releases/download/v${VERSION}`;
    const archiveUrl = `${baseUrl}/${archiveName}`;
    const shaUrl = `${archiveUrl}.sha256`;

    const archivePath = path.join(binDir, archiveName);
    const shaPath = `${archivePath}.sha256`;

    console.log(`Downloading argyph ${VERSION} for ${target}...`);

    try {
        await download(archiveUrl, archivePath);
    } catch (e) {
        console.error(`Failed to download ${archiveUrl}: ${e.message}`);
        console.error('Release may not be published yet for this version.');
        console.error('Install from source: cargo install argyph --locked');
        process.exit(1);
    }

    try {
        await download(shaUrl, shaPath);
        const expected = fs.readFileSync(shaPath, 'utf8').trim().split(/\s+/)[0];
        const actual = sha256File(archivePath);
        if (actual !== expected) {
            console.error(`SHA256 mismatch: expected ${expected}, got ${actual}`);
            fs.unlinkSync(archivePath);
            fs.unlinkSync(shaPath);
            process.exit(1);
        }
        fs.unlinkSync(shaPath);
        console.log('SHA256 verified.');
    } catch (e) {
        console.warn(`SHA256 verification skipped: ${e.message}`);
    }

    const extractDir = path.join(binDir, '_extract');
    fs.mkdirSync(extractDir, { recursive: true });
    try {
        extract(archivePath, extractDir, target);
    } catch (e) {
        console.error(`Failed to extract ${archivePath}: ${e.message}`);
        process.exit(1);
    }

    let foundBinary = null;
    function walk(dir) {
        for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
            const full = path.join(dir, entry.name);
            if (entry.isDirectory()) {
                walk(full);
            } else if (entry.name === binaryName) {
                foundBinary = full;
                return;
            }
        }
    }
    walk(extractDir);

    if (!foundBinary) {
        console.error(`Could not find ${binaryName} in extracted archive.`);
        process.exit(1);
    }

    fs.copyFileSync(foundBinary, finalBinary);
    if (!isWindowsTarget(target)) {
        fs.chmodSync(finalBinary, 0o755);
    }

    fs.rmSync(extractDir, { recursive: true, force: true });
    fs.unlinkSync(archivePath);

    console.log(`argyph ${VERSION} installed at ${finalBinary}`);
}

main().catch((err) => {
    console.error(`postinstall failed: ${err.message}`);
    process.exit(1);
});

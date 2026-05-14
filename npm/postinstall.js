#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const https = require('https');
const { execSync } = require('child_process');

const OWNER = 'Ezzy1630';
const REPO = 'argyph';
const VERSION = '0.3.0-beta';

function getPlatform() {
    const platform = process.platform;
    const arch = process.arch;
    
    if (platform === 'darwin' && arch === 'arm64') return 'aarch64-apple-darwin';
    if (platform === 'darwin' && arch === 'x64') return 'x86_64-apple-darwin';
    if (platform === 'linux' && arch === 'x64') return 'x86_64-unknown-linux-gnu';
    if (platform === 'linux' && arch === 'arm64') return 'aarch64-unknown-linux-gnu';
    if (platform === 'win32' && arch === 'x64') return 'x86_64-pc-windows-msvc';
    
    throw new Error(`Unsupported platform: ${platform} ${arch}`);
}

function getBinaryName(platform) {
    return platform.startsWith('x86_64-pc-windows') ? 'argyph.exe' : 'argyph';
}

function download(url, dest) {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        https.get(url, (response) => {
            if (response.statusCode === 302 || response.statusCode === 301) {
                download(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            if (response.statusCode !== 200) {
                reject(new Error(`Download failed: ${response.statusCode}`));
                return;
            }
            response.pipe(file);
            file.on('finish', () => {
                file.close();
                resolve();
            });
        }).on('error', reject);
    });
}

async function main() {
    const platform = getPlatform();
    const binaryName = getBinaryName(platform);
    const binDir = path.join(__dirname, 'bin');
    const dest = path.join(binDir, binaryName);
    
    if (fs.existsSync(dest)) {
        console.log(`argyph binary already installed at ${dest}`);
        return;
    }
    
    fs.mkdirSync(binDir, { recursive: true });
    
    const url = `https://github.com/${OWNER}/${REPO}/releases/download/v${VERSION}/${binaryName}-${platform}`;
    const fallbackUrl = `https://github.com/${OWNER}/${REPO}/releases/download/v${VERSION}/argyph-${platform}.tar.gz`;
    
    console.log(`Downloading argyph ${VERSION} for ${platform}...`);
    
    try {
        await download(url, dest);
    } catch (e) {
        console.error(`Failed to download from ${url}: ${e.message}`);
        console.error('Argyph release may not be available yet for this version.');
        console.error('Install from source: cargo install argyph');
        process.exit(1);
    }
    
    fs.chmodSync(dest, 0o755);
    console.log(`argyph ${VERSION} installed successfully`);
}

main().catch((err) => {
    console.error(`postinstall failed: ${err.message}`);
    process.exit(1);
});

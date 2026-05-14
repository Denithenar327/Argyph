#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');

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

const platform = getPlatform();
const binaryName = getBinaryName(platform);
const binaryPath = path.join(__dirname, binaryName);

try {
    const child = spawn(binaryPath, process.argv.slice(2), {
        stdio: 'inherit'
    });
    
    child.on('exit', (code) => {
        process.exit(code || 0);
    });
} catch (err) {
    console.error(`Failed to run argyph: ${err.message}`);
    console.error('Run npm install to download the binary.');
    process.exit(1);
}

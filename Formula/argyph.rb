class Argyph < Formula
  desc "Local-first MCP server giving AI coding agents fast, structured, and semantic context over any codebase"
  homepage "https://github.com/Ezzy1630/argyph"
  version "1.0.1"
  license "MIT OR Apache-2.0"

  # Prebuilt binaries from cargo-dist. SHA256 values are filled in
  # automatically by scripts/update-homebrew.sh after each tagged release.
  on_macos do
    on_arm do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.1/argyph-aarch64-apple-darwin.tar.xz"
      sha256 "ea8b48f68c20ae1c61c31dea9cbbff11aa69dbee04b3863b74671fa77a1bf13f"
    end
    # Intel Mac: no prebuilt available (ort/ONNX Runtime does not ship
    # an x86_64-apple-darwin binary). Fall back to building from source
    # via cargo. Homebrew will install rust as a build-time dependency.
  end

  on_linux do
    on_arm do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.1/argyph-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "ade04588a7a4bfddef6da89ae88d472dabaa84828340a9741c3e6432525410e9"
    end
    on_intel do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.1/argyph-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "7fa932b0538a4e614be973c912548a3ccddb066af7498adc42d6e774a9b917d6"
    end
  end

  def install
    if OS.mac? && Hardware::CPU.intel?
      odie <<~EOS
        Argyph does not ship a prebuilt binary for Intel macOS because the
        bundled ONNX Runtime backend has no x86_64-apple-darwin binary.
        Install via cargo instead:

            cargo install argyph --locked
      EOS
    end
    bin.install "argyph"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/argyph --version")
  end
end

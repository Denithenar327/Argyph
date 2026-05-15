class Argyph < Formula
  desc "Local-first MCP server giving AI coding agents fast, structured, and semantic context over any codebase"
  homepage "https://github.com/Ezzy1630/argyph"
  version "1.0.0-rc.2"
  license "MIT OR Apache-2.0"

  # Prebuilt binaries from cargo-dist. SHA256 values are filled in
  # automatically by scripts/update-homebrew.sh after each tagged release.
  on_macos do
    on_arm do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.0-rc.2/argyph-aarch64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_AARCH64_DARWIN_SHA256"
    end
    on_intel do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.0-rc.2/argyph-x86_64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_X86_64_DARWIN_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.0-rc.2/argyph-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_AARCH64_LINUX_SHA256"
    end
    on_intel do
      url "https://github.com/Ezzy1630/argyph/releases/download/v1.0.0-rc.2/argyph-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_X86_64_LINUX_SHA256"
    end
  end

  def install
    bin.install "argyph"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/argyph --version")
  end
end

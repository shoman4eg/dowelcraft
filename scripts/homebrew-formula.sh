#!/usr/bin/env bash
# Print the Homebrew formula for a release: homebrew-formula.sh <version> <dir with release archives>
set -euo pipefail
version="${1#v}"
dir="$2"
base="https://github.com/shoman4eg/dowelcraft/releases/download/v${version}"
sum() { sha256sum "$dir/dowel-craft-$1.tar.gz" | cut -d' ' -f1; }
cat <<RUBY
class DowelCraft < Formula
  desc "Cabinet furniture design: parts, sheet cutting, drilling, drawings"
  homepage "https://github.com/shoman4eg/dowelcraft"
  version "${version}"
  license "MIT"

  on_macos do
    on_arm do
      url "${base}/dowel-craft-macos-arm64.tar.gz"
      sha256 "$(sum macos-arm64)"
    end
    on_intel do
      url "${base}/dowel-craft-macos-x86_64.tar.gz"
      sha256 "$(sum macos-x86_64)"
    end
  end

  on_linux do
    on_intel do
      url "${base}/dowel-craft-linux-x86_64.tar.gz"
      sha256 "$(sum linux-x86_64)"
    end
  end

  def install
    bin.install "dowel-craft-gui", "dowel-craft", "dowel-craft-mcp"
  end

  test do
    assert_match "dowel-craft", shell_output("#{bin}/dowel-craft 2>&1", 1)
  end
end
RUBY

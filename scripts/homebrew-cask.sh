#!/usr/bin/env bash
# Print the Homebrew cask (macOS app with icon) for a release: homebrew-cask.sh <version> <dir with release archives>
set -euo pipefail
version="${1#v}"
dir="$2"
sum() { sha256sum "$dir/dowel-craft-$1.tar.gz" | cut -d' ' -f1; }
cat <<RUBY
cask "dowel-craft-app" do
  arch arm: "arm64", intel: "x86_64"

  version "${version}"
  sha256 arm:   "$(sum macos-arm64)",
         intel: "$(sum macos-x86_64)"

  url "https://github.com/shoman4eg/dowelcraft/releases/download/v#{version}/dowel-craft-macos-#{arch}.tar.gz"
  name "Dowel Craft"
  desc "Cabinet furniture design: parts, sheet cutting, drilling, drawings"
  homepage "https://github.com/shoman4eg/dowelcraft"

  depends_on macos: ">= :big_sur"

  app "dowel-craft-macos-#{arch}/Dowel Craft.app"

  caveats <<~EOS
    Dowel Craft is not notarized by Apple yet. If macOS refuses to open it, run:
      xattr -dr com.apple.quarantine "#{appdir}/Dowel Craft.app"
  EOS
end
RUBY

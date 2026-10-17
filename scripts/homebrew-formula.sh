#!/usr/bin/env bash
set -euo pipefail

version=${1:?expected a release version}
checksums=${2:?expected SHA256SUMS}

if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  printf 'Expected a stable release version, got %s.\n' "$version" >&2
  exit 1
fi

checksum() {
  local name="cinekey-$version-$1" value
  value=$(awk -v name="$name" '$2 == name { print $1 }' "$checksums")
  if [[ ! $value =~ ^[0-9a-f]{64}$ ]]; then
    printf 'Expected one SHA-256 checksum for %s.\n' "$name" >&2
    exit 1
  fi
  printf '%s' "$value"
}

mac_arm=$(checksum aarch64-apple-darwin)
mac_intel=$(checksum x86_64-apple-darwin)
linux_arm=$(checksum aarch64-unknown-linux-gnu)
linux_intel=$(checksum x86_64-unknown-linux-gnu)

cat <<EOF
class Cinekey < Formula
  desc "Fast parser for movie and TV release names"
  homepage "https://github.com/Sec-ant/cinekey"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Sec-ant/cinekey/releases/download/v$version/cinekey-$version-aarch64-apple-darwin",
          using: :nounzip
      sha256 "$mac_arm"
    end

    on_intel do
      url "https://github.com/Sec-ant/cinekey/releases/download/v$version/cinekey-$version-x86_64-apple-darwin",
          using: :nounzip
      sha256 "$mac_intel"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Sec-ant/cinekey/releases/download/v$version/cinekey-$version-aarch64-unknown-linux-gnu",
          using: :nounzip
      sha256 "$linux_arm"
    end

    on_intel do
      url "https://github.com/Sec-ant/cinekey/releases/download/v$version/cinekey-$version-x86_64-unknown-linux-gnu",
          using: :nounzip
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install File.basename(stable.url) => "cinekey"
    (bin/"cinekey").chmod 0755
  end

  test do
    output = shell_output("#{bin}/cinekey --json Young.Justice.S02.1080p.Blu-ray.DD2.0.Hi10P.x264-CtrlHD")
    media = JSON.parse(output)
    assert_equal "Young Justice", media.fetch("title")
    assert_equal 2, media.fetch("season")
    assert_equal 10, media.fetch("video_bit_depth")
    assert_equal "CtrlHD", media.fetch("release_group")
  end
end
EOF

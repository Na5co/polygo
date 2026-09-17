# Homebrew formula for the tap `atanasa/homebrew-tap` (brew install atanasa/tap/polygo).
# Release automation fills in the version and sha256 values from SHA256SUMS.
class Polygo < Formula
  desc "Lokalise for one person: local-first, git-native localization CLI"
  homepage "https://github.com/atanasa/polygo"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/atanasa/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_aarch64-darwin"
    end
    on_intel do
      url "https://github.com/atanasa/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_x86_64-darwin"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/atanasa/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-linux.tar.gz"
      sha256 "REPLACE_WITH_SHA256_aarch64-linux"
    end
    on_intel do
      url "https://github.com/atanasa/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-linux.tar.gz"
      sha256 "REPLACE_WITH_SHA256_x86_64-linux"
    end
  end

  def install
    bin.install "polygo"
  end

  test do
    assert_match "polygo", shell_output("#{bin}/polygo --version")
  end
end

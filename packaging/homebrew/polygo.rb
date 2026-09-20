# Homebrew formula for the tap `Na5co/homebrew-tap` (brew install na5co/tap/polygo).
# Release automation fills in the version and sha256 values from SHA256SUMS.
class Polygo < Formula
  desc "Translate app strings (xcstrings, Android, ARB, i18next, po, resx) via local LLM"
  homepage "https://github.com/Na5co/polygo"
  version "0.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-darwin.tar.gz"
      sha256 "edac8a1b0dcfce58b4edaffd0b7f9772dc22685482941301263d6c0abbb55a1a"
    end
    on_intel do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-darwin.tar.gz"
      sha256 "274364cc4dc60357d2cc276019a1d00af3cd032f83c8ba921c16d613152fd023"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-linux.tar.gz"
      sha256 "f92dd89226f7db019551782911df1f56698b66814c8d4b08b35e52e3a35b6952"
    end
    on_intel do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-linux.tar.gz"
      sha256 "7473b50be399c155d438317a64954a7dd87cf94abf73985abbf871667a8fa236"
    end
  end

  def install
    bin.install "polygo"
    # From the first release after 0.1.1 (`polygo completions` exists), also:
    # generate_completions_from_executable(bin/"polygo", "completions")
  end

  test do
    assert_match "polygo", shell_output("#{bin}/polygo --version")
  end
end

# Homebrew formula for the tap `Na5co/homebrew-tap` (brew install na5co/tap/polygo).
# Release automation fills in the version and sha256 values from SHA256SUMS.
class Polygo < Formula
  desc "Translate app strings (xcstrings, Android, ARB, i18next, po, resx) via local LLM"
  homepage "https://github.com/Na5co/polygo"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-darwin.tar.gz"
      sha256 "7a36ef2bd12092bb381c01c349e0d43810056e75b0cd873b7230e1e718a88642"
    end
    on_intel do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-darwin.tar.gz"
      sha256 "1d05bb8ce92337babda85e116cca622e2774ae8baec702f53acc09dacfabc160"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-aarch64-linux.tar.gz"
      sha256 "d3c007f9aa2033dafb1a7faff1501f2399e97ac54aee90100e60acb2da9c9eec"
    end
    on_intel do
      url "https://github.com/Na5co/polygo/releases/download/v#{version}/polygo-v#{version}-x86_64-linux.tar.gz"
      sha256 "a5c6127c91c4812f9094002793b49a6afa7c7ebc6c24c8b257ab7c23ad673078"
    end
  end

  def install
    bin.install "polygo"
  end

  test do
    assert_match "polygo", shell_output("#{bin}/polygo --version")
  end
end

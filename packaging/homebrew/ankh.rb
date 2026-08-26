# Homebrew formula (tap: KadenThomp36/homebrew-ankh). Regenerate checksums
# with scripts/update-packaging.sh <version>.
class Ankh < Formula
  desc "Neovim-flavoured Anki client for the terminal"
  homepage "https://github.com/KadenThomp36/ankh"
  version "0.1.0"
  license "AGPL-3.0-or-later"

  on_macos do
    on_arm do
      url "https://github.com/KadenThomp36/ankh/releases/download/v#{version}/ankh-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "SKIP"
    end
    on_intel do
      url "https://github.com/KadenThomp36/ankh/releases/download/v#{version}/ankh-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "SKIP"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/KadenThomp36/ankh/releases/download/v#{version}/ankh-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "SKIP"
    end
    on_intel do
      url "https://github.com/KadenThomp36/ankh/releases/download/v#{version}/ankh-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "SKIP"
    end
  end

  def install
    bin.install "ankh"
    generate_completions_from_executable(bin/"ankh", "completions")
  end

  test do
    assert_match "ankh", shell_output("#{bin}/ankh --version")
  end
end

class Flume < Formula
  desc "Modern terminal IRC client with scripting and LLM support"
  homepage "https://github.com/FlumeIRC/flume"
  url "https://github.com/FlumeIRC/flume/archive/refs/tags/v1.1.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "BSD-3-Clause"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: "flume-tui")
    bin.install "target/release/flume"
    man1.install "doc/flume.1"
  end

  test do
    assert_match "Flume", shell_output("#{bin}/flume --version 2>&1", 1)
  end
end

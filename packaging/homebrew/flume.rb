class Flume < Formula
  desc "Modern terminal IRC client with scripting and LLM support"
  homepage "https://github.com/FlumeIRC/flume"
  url "https://github.com/FlumeIRC/flume/archive/refs/tags/v1.2.0.tar.gz"
  sha256 "PLACEHOLDER_SHA256"
  license "Apache-2.0"

  depends_on "rust" => :build
  depends_on "python@3" => :recommended

  def install
    features = []
    features << "python" if build.with?("python@3")

    args = std_cargo_args(path: "flume-tui")
    args += ["--features", features.join(",")] unless features.empty?

    ENV["PYO3_USE_ABI3_FORWARD_COMPATIBILITY"] = "1" if build.with?("python@3")

    system "cargo", "install", *args
    bin.install "target/release/flume"
    man1.install "doc/flume.1"
  end

  test do
    assert_match "Flume", shell_output("#{bin}/flume --version 2>&1", 1)
  end
end

class Chattor < Formula
  desc "Privacy-first TUI chat application over Tor"
  homepage "https://github.com/jmsingleton/chattor"
  url "https://github.com/jmsingleton/chattor/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "PLACEHOLDER"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on "rust" => :build

  def install
    cd "chattor" do
      system "cargo", "install", *std_cargo_args
      man1.install "man/chattor.1"
      bash_completion.install "completions/chattor.bash" => "chattor"
      zsh_completion.install "completions/_chattor"
      fish_completion.install "completions/chattor.fish"
    end
  end

  test do
    assert_match "chattor", shell_output("#{bin}/chattor --help")
  end
end

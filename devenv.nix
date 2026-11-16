{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  languages = {
    c.enable = true;
    rust.enable = true;
  };

  packages = with pkgs; [alejandra bat cargo-audit cargo-deny];

  enterTest = ''
    cargo test
    cargo clippy
    cargo deny check
    cargo audit -f ${./Cargo.nix.lock} --json | ${lib.getExe pkgs.jq} -e '. as $expression | $expression, ($expression | .vulnerabilities.found | not)'
  '';

  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
    trufflehog.enable = true;
  };

  outputs = {
    datadis = pkgs.rustPlatform.buildRustPackage {
      name = "datadis";
      cargoLock.lockFile = ./Cargo.nix.lock;
      postPatch = ''
        ln -sf ${./Cargo.nix.lock} Cargo.lock
      '';
      buildPhase = ''
        runHook preBuild
        cargo build --release --examples --bins
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p $out/bin
        cp target/release/datadis $out/bin/datadis
        ls -1 $src/examples | sed 's/\.rs$//' | \
          xargs -I{} sh -c 'cp target/release/examples/{} $out/bin/$(echo {} | sed 's/_/-/g')'
        runHook postInstall
      '';
      src = ./.;
    };
  };
}

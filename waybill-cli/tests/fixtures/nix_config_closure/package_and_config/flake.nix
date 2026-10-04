# m1066 (#1052): a package flake that also defines a system configuration.
# The package output wins on every host (FR-002, US3); the configuration is
# never selected automatically. No inputs; never built.
{
  outputs = { self }: let
    mk = name: system: derivation {
      inherit name system;
      builder = "/bin/sh";
      args = [ "-c" "echo > $out" ];
    };
    systems = [ "aarch64-darwin" "x86_64-darwin" "aarch64-linux" "x86_64-linux" ];
  in {
    packages = builtins.listToAttrs (map (s: {
      name = s;
      value.default = mk "hello-1.0" s;
    }) systems);
    darwinConfigurations.laptop.system = mk "darwin-system-laptop-1.0" "aarch64-darwin";
  };
}

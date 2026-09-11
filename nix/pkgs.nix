# The one `nixpkgs` instantiation every other module in this repo receives.
#
# It exists as a module rather than a `let` binding because the overlay set is a
# policy decision, and a policy that lives in two places is a policy that will
# disagree with itself.
{
  nixpkgs,
  system,
  rust-overlay,
}:
import nixpkgs {
  inherit system;
  overlays = [ (import rust-overlay) ];
}

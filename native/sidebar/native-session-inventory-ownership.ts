export type NativeSessionInventoryOwnership = "gxserver" | "local";

export type NativeSessionInventoryOwnershipFields = {
  isLocalOnly: boolean;
  ownership: NativeSessionInventoryOwnership;
};

export type NativeSessionInventoryOwnershipInput = {
  hasGxserverProjectContext?: boolean;
  hasGxserverSessionReference?: boolean;
};

/*
CDXC:SessionInventoryOwnership 2026-06-02-17:19:
Agent Manager, gx CLI bridge responses, and Running Sessions can include macOS-local panes alongside gxserver-backed sessions. Every emitted inventory row must carry explicit ownership so local panes cannot be mistaken for shared gxserver sessions after presentation initializes.
*/
export function resolveNativeSessionInventoryOwnership({
  hasGxserverProjectContext = false,
  hasGxserverSessionReference = false,
}: NativeSessionInventoryOwnershipInput): NativeSessionInventoryOwnershipFields {
  const ownership: NativeSessionInventoryOwnership =
    hasGxserverProjectContext || hasGxserverSessionReference ? "gxserver" : "local";
  return {
    isLocalOnly: ownership === "local",
    ownership,
  };
}

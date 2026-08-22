import { Button } from "@/packages/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";

export type RemoteGxserverInstallModalProps = {
  isOpen: boolean;
  machineName: string;
  onApprove: () => void;
  onCancel: () => void;
};

export function RemoteGxserverInstallModal({
  isOpen,
  machineName,
  onApprove,
  onCancel,
}: RemoteGxserverInstallModalProps) {
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          onCancel();
        }
      }}
      open={isOpen}
    >
      {/*
       * CDXC:RemoteMachines 2026-06-02-23:38:
       * Missing remote gxserver installation must be a user-approved React
       * modal. The app explains why gxserver is required before native uploads
       * the bundled package over SSH and starts it on the selected machine.
       *
       * CDXC:RemoteMachines 2026-06-23-09:46:
       * The approved install path must use a package that matches the remote
       * machine's OS and CPU. The package owns gxserver plus the pinned zmx,
       * bd, Ghostex CLI, and Node resources needed for first-run Ubuntu
       * attach, so copy must not imply that only the local macOS daemon is
       * uploaded.
       */}
      <DialogContent className="remote-gxserver-install-modal" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle className="text-xl">Install remote gxserver</DialogTitle>
        </DialogHeader>
        <div className="remote-gxserver-install-modal-body">
          <p>
            Ghostex can connect to {machineName}, but gxserver is not installed there. Ghostex needs gxserver on that machine to browse folders, add projects, clone repositories, and manage sessions remotely.
          </p>
          <p>
            If you continue, Ghostex will copy its compatible bundled remote package over SSH into{" "}
            <code>{"${XDG_DATA_HOME:-~/.local/share}/ghostex/gxserver"}</code>, expose <code>gxserver</code>, <code>zmx</code>,{" "}
            <code>bd</code>, <code>ghostex</code>, and <code>gx</code> from{" "}
            <code>~/.local/bin</code> when possible, start gxserver, then connect through an SSH
            tunnel. Windows machines use the selected or default WSL2 distribution, and Ghostex
            installs the Linux package in that distribution&apos;s home directory.
          </p>
        </div>
        <div className="remote-gxserver-install-modal-actions">
          <Button onClick={onCancel} type="button" variant="outline">
            Cancel
          </Button>
          <Button onClick={onApprove} type="button">
            Install gxserver
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

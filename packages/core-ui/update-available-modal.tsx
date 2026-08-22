import { Button } from "@/packages/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import ReactMarkdown from "react-markdown";

export type UpdateAvailableModalState = {
  notesMarkdown: string;
  portable: boolean;
  state: "available" | "ready";
  version: string;
};

export function UpdateAvailableModal({
  isOpen,
  onCancel,
  onDownload,
  onRestart,
  update,
}: {
  isOpen: boolean;
  onCancel: () => void;
  onDownload: () => void;
  onRestart: () => void;
  update?: UpdateAvailableModalState;
}) {
  const ready = update?.state === "ready";
  return (
    <Dialog
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onCancel();
      }}
      open={isOpen}
    >
      <DialogContent className="update-available-modal" showCloseButton={false}>
        <DialogHeader>
          <DialogTitle className="text-xl">
            {ready
              ? "Ghostex is ready to update"
              : "A Ghostex update is available"}
          </DialogTitle>
        </DialogHeader>
        <p className="update-available-modal-version">
          Version {update?.version}
        </p>
        <div className="update-available-modal-notes">
          {update?.notesMarkdown.trim() ? (
            <ReactMarkdown
              components={{
                a: ({ children }) => <span>{children}</span>,
                img: () => null,
              }}
              skipHtml
            >
              {update.notesMarkdown}
            </ReactMarkdown>
          ) : (
            <p>This update includes improvements and fixes for Ghostex.</p>
          )}
        </div>
        {update?.portable ? (
          <p className="update-available-modal-portable">
            This portable copy will be updated in place and remain portable.
          </p>
        ) : null}
        <div className="update-available-modal-actions">
          <Button onClick={onCancel} type="button" variant="outline">
            {ready ? "Later" : "Cancel"}
          </Button>
          <Button onClick={ready ? onRestart : onDownload} type="button">
            {ready ? "Restart and update" : "Download update"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

import {
  useMemo,
  useState,
} from "react";
import { Button } from "@/packages/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/packages/components/ui/dialog";
import { Input } from "@/packages/components/ui/input";
import {
  beadsErrorMessage,
  boardColumnNameError,
  boardStatusLabel,
  managedBoardColumnNames,
  ensureWorkflowStatuses,
  type BoardColumn,
  type BoardTicket,
} from "../project-board-shared";

/*
  CDXC:ProjectBoardColumnManagement 2026-08-21:
  The dialog only ever offers the board's own extra statuses. The six built-in lanes are listed but
  locked, because they are reconciled into the config by ensureWorkflowStatuses on every load and
  renaming or removing one here would simply be undone on the next refresh.
  Deleting is disabled while a column still holds beads and says how many are in the way, rather than
  moving them somewhere on the user's behalf: an unconfigured status renders as Todo, so an automatic
  sweep would turn parked work into work that looks fresh.
*/
export function BoardColumnsDialog({
  columns,
  config,
  onClose,
  onCreate,
  onDelete,
  onRename,
  onReorder,
  open,
  tickets,
}: {
  columns: BoardColumn[];
  config: string;
  onClose: () => void;
  onCreate: (name: string) => Promise<void>;
  onDelete: (name: string) => Promise<void>;
  onRename: (from: string, to: string) => Promise<void>;
  onReorder: (name: string, delta: -1 | 1) => Promise<void>;
  open: boolean;
  tickets: BoardTicket[];
}) {
  const [draftName, setDraftName] = useState("");
  const [renamingName, setRenamingName] = useState("");
  const [renameDraft, setRenameDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  const managedNames = useMemo(() => managedBoardColumnNames(config), [config]);
  const builtinColumns = useMemo(
    () => columns.filter((column) => !managedNames.includes(String(column.key))),
    [columns, managedNames],
  );
  const ticketCountByStatus = useMemo(() => {
    const counts = new Map<string, number>();
    for (const ticket of tickets) {
      const key = String(ticket.boardStatus);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  }, [tickets]);

  const closeDialog = () => {
    setDraftName("");
    setRenamingName("");
    setRenameDraft("");
    setErrorMessage("");
    onClose();
  };

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setErrorMessage("");
    try {
      await action();
    } catch (error) {
      setErrorMessage(beadsErrorMessage(error instanceof Error ? error.message : ""));
    } finally {
      setBusy(false);
    }
  };

  const createError = draftName.trim() ? boardColumnNameError(draftName, config) : "";
  const renameError =
    renamingName && renameDraft.trim() && renameDraft.trim() !== renamingName
      ? boardColumnNameError(renameDraft, config)
      : "";

  return (
    <Dialog onOpenChange={(next) => (next ? undefined : closeDialog())} open={open}>
      <DialogContent className="project-ticket-dialog project-board-columns-dialog gap-4 p-5">
        <DialogHeader className="gap-1">
          <DialogTitle className="text-[15px] font-normal">Board columns</DialogTitle>
          <DialogDescription className="text-xs font-normal text-muted-foreground">
            Extra columns come from this board&apos;s Beads status config. The six built-in lanes are fixed.
          </DialogDescription>
        </DialogHeader>
        <div className="project-ticket-dialog-body vertical-scroll-fade-mask">
          <ul className="project-board-columns-list">
            {builtinColumns.map((column) => (
              <li className="project-board-columns-row" data-locked="true" key={String(column.key)}>
                <span className="project-board-columns-name">{column.label}</span>
                <span className="project-board-columns-note">Built-in</span>
              </li>
            ))}
            {managedNames.map((name, index) => {
              const ticketCount = ticketCountByStatus.get(name) ?? 0;
              const isRenaming = renamingName === name;
              return (
                <li className="project-board-columns-row" key={name}>
                  {isRenaming ? (
                    <>
                      <Input
                        aria-label={`Rename ${name}`}
                        autoFocus
                        onChange={(event) => setRenameDraft(event.currentTarget.value)}
                        value={renameDraft}
                      />
                      <Button
                        disabled={busy || Boolean(renameError) || !renameDraft.trim()}
                        onClick={() =>
                          void run(async () => {
                            if (renameDraft.trim() !== name) {
                              await onRename(name, renameDraft);
                            }
                            setRenamingName("");
                          })
                        }
                      >
                        Save
                      </Button>
                      <Button onClick={() => setRenamingName("")} variant="ghost">
                        Cancel
                      </Button>
                    </>
                  ) : (
                    <>
                      <span className="project-board-columns-name">{boardStatusLabel(name, columns)}</span>
                      <span className="project-board-columns-note">
                        {ticketCount === 1 ? "1 card" : `${ticketCount} cards`}
                      </span>
                      <Button
                        aria-label={`Move ${name} up`}
                        disabled={busy || index === 0}
                        onClick={() => void run(() => onReorder(name, -1))}
                        variant="ghost"
                      >
                        ↑
                      </Button>
                      <Button
                        aria-label={`Move ${name} down`}
                        disabled={busy || index === managedNames.length - 1}
                        onClick={() => void run(() => onReorder(name, 1))}
                        variant="ghost"
                      >
                        ↓
                      </Button>
                      <Button
                        disabled={busy}
                        onClick={() => {
                          setRenamingName(name);
                          setRenameDraft(name);
                        }}
                        variant="ghost"
                      >
                        Rename
                      </Button>
                      <Button
                        disabled={busy || ticketCount > 0}
                        onClick={() => void run(() => onDelete(name))}
                        title={
                          ticketCount > 0
                            ? `Move its ${ticketCount === 1 ? "card" : "cards"} out first.`
                            : undefined
                        }
                        variant="ghost"
                      >
                        Delete
                      </Button>
                    </>
                  )}
                </li>
              );
            })}
          </ul>
          {renameError ? <p className="project-board-columns-error">{renameError}</p> : null}
          <div className="project-board-columns-add">
            <Input
              aria-label="New column name"
              onChange={(event) => setDraftName(event.currentTarget.value)}
              placeholder="New column name"
              value={draftName}
            />
            <Button
              disabled={busy || Boolean(createError) || !draftName.trim()}
              onClick={() =>
                void run(async () => {
                  await onCreate(draftName);
                  setDraftName("");
                })
              }
            >
              Add column
            </Button>
          </div>
          {createError ? <p className="project-board-columns-error">{createError}</p> : null}
          {errorMessage ? <p className="project-board-columns-error">{errorMessage}</p> : null}
        </div>
        <DialogFooter>
          <Button onClick={closeDialog} variant="outline">
            Done
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
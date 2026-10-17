"use client";
import * as Dialog from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import clsx from "clsx";
import { Toaster, toast } from "sonner";

export function DialogDemo() {
  return (
    <>
    <Toaster />
    <button className="toast-trigger" onClick={() => toast("Saved with sonner")}>Show toast</button>
    <Dialog.Root>
      <Dialog.Trigger className={clsx("trigger", { active: true })}>Open dialog</Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="overlay" />
        <Dialog.Content className="content">
          <Dialog.Title>Edit profile</Dialog.Title>
          <Dialog.Description>Radix focus scope, dismissable layer and portal are working.</Dialog.Description>
          <Dialog.Close aria-label="Close">
            <X size={16} />
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
    </>
  );
}

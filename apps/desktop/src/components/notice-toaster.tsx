import { useEffect, useRef } from "react";
import { toast } from "sonner";
import { useAppStore } from "../stores/app.store";

/** Bridges store notices to the shadcn Sonner toaster. Renders nothing itself. */
export function NoticeToaster() {
  const notice = useAppStore((store) => store.notice);
  const noticeVariant = useAppStore((store) => store.noticeVariant);
  const noticeSeq = useAppStore((store) => store.noticeSeq);
  const lastSeq = useRef(noticeSeq);

  useEffect(() => {
    if (notice === null || noticeSeq === 0 || noticeSeq === lastSeq.current) return;
    lastSeq.current = noticeSeq;
    if (noticeVariant === "success") toast.success(notice);
    else if (noticeVariant === "error") toast.error(notice);
    else if (noticeVariant === "loading") toast.loading(notice);
    else toast.info(notice);
  }, [notice, noticeSeq, noticeVariant]);

  return null;
}

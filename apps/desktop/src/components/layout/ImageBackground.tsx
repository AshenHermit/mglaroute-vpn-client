import { useEffect, useMemo, useRef } from "react";

/**
 * Full-window parallax backdrop. Tauri has no document scroll, so the offset
 * comes from the pointer and the center zone's own scroll position.
 */
export function ImageBackground() {
  const ref = useRef<HTMLDivElement>(null);
  const imageIndex = useMemo(() => 1 + Math.floor(Math.random() * 3), []);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    const target = { x: 0, y: 0, scroll: 0 };
    const current = { x: 0, y: 0, scroll: 0 };
    let frame: number | undefined;

    const paint = () => {
      current.x += (target.x - current.x) * 0.12;
      current.y += (target.y - current.y) * 0.12;
      current.scroll += (target.scroll - current.scroll) * 0.12;
      element.style.transform = `translate3d(${current.x.toFixed(2)}px, ${(
        current.y + current.scroll
      ).toFixed(2)}px, 0)`;
      frame = undefined;
    };

    const schedule = () => {
      if (frame === undefined) frame = requestAnimationFrame(paint);
    };

    const onPointerMove = (event: PointerEvent) => {
      target.x = (0.5 - event.clientX / window.innerWidth) * 24;
      target.y = (0.5 - event.clientY / window.innerHeight) * 24;
      schedule();
    };

    const onCenterScroll = (event: Event) => {
      const zone = event.currentTarget as HTMLElement;
      target.scroll = -zone.scrollTop * 0.08;
      schedule();
    };

    const centerZone = document.querySelector<HTMLElement>('[data-testid="center-zone"]');
    window.addEventListener("pointermove", onPointerMove);
    centerZone?.addEventListener("scroll", onCenterScroll, { passive: true });

    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      centerZone?.removeEventListener("scroll", onCenterScroll);
      if (frame !== undefined) cancelAnimationFrame(frame);
    };
  }, []);

  return (
    <div
      ref={ref}
      data-testid="app-background"
      aria-hidden="true"
      className="pointer-events-none fixed inset-0 -z-10 bg-cover bg-center bg-no-repeat will-change-transform"
      style={{ backgroundImage: `url(/bg/bg_${imageIndex}.jpg)` }}
    />
  );
}

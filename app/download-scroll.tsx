'use client';

import { useEffect } from 'react';

export default function DownloadScroll() {
  useEffect(() => {
    let following = false;
    let frame = 0;
    const bottom = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (following)
          window.scrollTo({
            top: document.documentElement.scrollHeight,
            behavior: 'instant',
          });
      });
    };
    const navigate = () => {
      following = window.location.hash === '#download';
      if (following) bottom();
    };
    const click = (event: MouseEvent) => {
      if (
        event.defaultPrevented ||
        event.button !== 0 ||
        event.metaKey ||
        event.ctrlKey ||
        event.shiftKey ||
        event.altKey
      )
        return;
      const link =
        event.target instanceof Element ? event.target.closest('a') : null;
      if (link?.getAttribute('href') === '#download') {
        following = true;
        bottom();
      }
    };
    const stop = () => {
      following = false;
    };
    const key = (event: KeyboardEvent) => {
      if (
        [
          'ArrowUp',
          'ArrowDown',
          'PageUp',
          'PageDown',
          'Home',
          'End',
          ' ',
        ].includes(event.key)
      )
        stop();
    };
    const resize = new ResizeObserver(() => {
      if (following) bottom();
    });
    resize.observe(document.body);
    window.addEventListener('hashchange', navigate);
    document.addEventListener('click', click);
    window.addEventListener('wheel', stop, { passive: true });
    window.addEventListener('touchstart', stop, { passive: true });
    window.addEventListener('keydown', key);
    navigate();
    return () => {
      resize.disconnect();
      cancelAnimationFrame(frame);
      window.removeEventListener('hashchange', navigate);
      document.removeEventListener('click', click);
      window.removeEventListener('wheel', stop);
      window.removeEventListener('touchstart', stop);
      window.removeEventListener('keydown', key);
    };
  }, []);
  return null;
}

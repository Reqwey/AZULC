'use client';

import { useEffect } from 'react';

/** Progressive enhancement: content stays visible without JavaScript. */
export default function Motion() {
  useEffect(() => {
    const preference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const animations = new Set<Animation>();
    let observer: IntersectionObserver | undefined;

    function configure() {
      observer?.disconnect();
      animations.forEach((animation) => animation.cancel());
      animations.clear();
      if (preference.matches || !('IntersectionObserver' in window)) return;

      observer = new IntersectionObserver(
        (entries) => {
          entries.forEach((entry) => {
            if (!entry.isIntersecting) return;
            observer?.unobserve(entry.target);
            const animation = entry.target.animate(
              [
                { opacity: 0, transform: 'translateY(24px)' },
                { opacity: 1, transform: 'translateY(0)' },
              ],
              { duration: 650, easing: 'cubic-bezier(0.2, 0.7, 0.2, 1)' },
            );
            animations.add(animation);
            animation.onfinish = () => animations.delete(animation);
          });
        },
        { threshold: 0.08 },
      );

      document
        .querySelectorAll(
          '.section-heading, .feature-copy, .feature .screen, .details article, .about > *, .download-content',
        )
        .forEach((element) => {
          // Do not animate content already in view, including deep-linked sections.
          if (element.getBoundingClientRect().top >= window.innerHeight) {
            observer?.observe(element);
          }
        });
    }

    configure();
    preference.addEventListener('change', configure);
    return () => {
      observer?.disconnect();
      animations.forEach((animation) => animation.cancel());
      preference.removeEventListener('change', configure);
    };
  }, []);

  return null;
}

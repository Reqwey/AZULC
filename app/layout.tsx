import type { Metadata } from 'next';
import './globals.css';
export const metadata: Metadata = {
  title: 'AZULC · Azusa Minecraft Launcher',
  description:
    'A native Minecraft launcher built in Rust. Create isolated instances, discover modpacks, manage mods, and follow installations and launches with live logs.',
  icons: { icon: '/assets/app-icon.png' },
};
export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}

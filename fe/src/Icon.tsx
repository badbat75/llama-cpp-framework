// One Lucide icon (fe/briefs/08-hearth-h8-icons.md section 0): the eight
// attributes of the svg around the shapes of the brief's list, so a placement
// names the icon and repeats nothing. Every icon is decorative: it sits beside
// text that says the same, so it carries no title, no label, no role and no
// size of its own, the stylesheet sizing it. The shapes are Lucide's path data
// (ISC), copied as data: the licence text ships with the installer.
import { type ReactNode } from 'react'

/** Every name of the brief's list, the shapes of the later slices included: a
 * slice that adds a placement adds no shape, beside the shapes a brief gives
 * (22 D2.2's minus). */
export type IconName =
  | 'house'
  | 'layers'
  | 'menu'
  | 'moon'
  | 'log-out'
  | 'play'
  | 'square'
  | 'rotate-cw'
  | 'chevron-left'
  | 'chevron-down'
  | 'search'
  | 'info'
  // 18 AN.6 (fe/briefs/18-about-node-info.md): the Node info nav entry.
  | 'cpu'
  | 'computer'
  // 20 N.4 (fe/briefs/20-nodes-menu.md): the host label of the Nodes group. The
  // citation follows its name, where 20 N.11 reads the text after it.
  | 'box'
  // 20 N.5 (fe/briefs/20-nodes-menu.md): every instance link of the host list.
  // 16 17a.2 (fe/briefs/16-server-profiles-and-instances.md, decision 1): the
  // Server profiles entry of the Profiles group.
  | 'server'
  | 'triangle-alert'
  | 'lock-open'
  | 'circle-alert'
  | 'plus'
  // 22 D2.2 (fe/briefs/22-model-sources.md, section 22D2): the delete of a
  // profile, the horizontal stroke of the plus with its vertical one dropped.
  | 'minus'
  | 'x'
  | 'check'
  // U17c (fe/briefs/17-ux-pass.md): the arrows of the sortable table's headers,
  // the up-and-down one while the column is not sorted.
  | 'arrow-up'
  | 'arrow-down'
  | 'arrow-up-down'

/** The children of the svg, the shapes of the brief's list, in its order. */
const SHAPES: Record<IconName, ReactNode> = {
  house: (
    <>
      <path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" />
      <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </>
  ),
  layers: (
    <>
      <path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83z" />
      <path d="M2 12a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 12" />
      <path d="M2 17a1 1 0 0 0 .58.91l8.6 3.91a2 2 0 0 0 1.65 0l8.58-3.9A1 1 0 0 0 22 17" />
    </>
  ),
  menu: (
    <>
      <path d="M4 5h16" />
      <path d="M4 12h16" />
      <path d="M4 19h16" />
    </>
  ),
  moon: (
    <path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401" />
  ),
  'log-out': (
    <>
      <path d="m16 17 5-5-5-5" />
      <path d="M21 12H9" />
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
    </>
  ),
  play: (
    <path d="M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z" />
  ),
  square: <rect width="18" height="18" x="3" y="3" rx="2" />,
  'rotate-cw': (
    <>
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
    </>
  ),
  'chevron-left': <path d="m15 18-6-6 6-6" />,
  'chevron-down': <path d="m6 9 6 6 6-6" />,
  search: (
    <>
      <path d="m21 21-4.34-4.34" />
      <circle cx="11" cy="11" r="8" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="10" />
      <path d="M12 16v-4" />
      <path d="M12 8h.01" />
    </>
  ),
  cpu: (
    <>
      <path d="M12 20v2" />
      <path d="M12 2v2" />
      <path d="M17 20v2" />
      <path d="M17 2v2" />
      <path d="M2 12h2" />
      <path d="M2 17h2" />
      <path d="M2 7h2" />
      <path d="M20 12h2" />
      <path d="M20 17h2" />
      <path d="M20 7h2" />
      <path d="M7 20v2" />
      <path d="M7 2v2" />
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <rect x="8" y="8" width="8" height="8" rx="1" />
    </>
  ),
  // 20 N.4 (fe/briefs/20-nodes-menu.md): the host label. The path data is
  // lucide-static 1.74.0 icons/computer.svg, the machine the brief's decision 7
  // picks for the host entry.
  computer: (
    <>
      <path d="M12 18h6" />
      <path d="M6 18h.01" />
      <path d="M8 6h1" />
      <rect x="2" y="14" width="20" height="8" rx="2" />
      <rect x="4" y="2" width="16" height="12" rx="2" />
    </>
  ),
  // 20 N.5 (fe/briefs/20-nodes-menu.md): each instance link, one running unit on
  // the host. The path data is lucide-static 1.74.0 icons/box.svg.
  box: (
    <>
      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
      <path d="m3.3 7 8.7 5 8.7-5" />
      <path d="M12 22V12" />
    </>
  ),
  // 16 17a.2 (fe/briefs/16-server-profiles-and-instances.md, decision 1): the
  // Server profiles entry. The path data is lucide-static 1.74.0
  // icons/server.svg.
  server: (
    <>
      <rect width="20" height="8" x="2" y="2" rx="2" ry="2" />
      <rect width="20" height="8" x="2" y="14" rx="2" ry="2" />
      <line x1="6" x2="6.01" y1="6" y2="6" />
      <line x1="6" x2="6.01" y1="18" y2="18" />
    </>
  ),
  'triangle-alert': (
    <>
      <path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" />
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
    </>
  ),
  'lock-open': (
    <>
      <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
      <path d="M7 11V7a5 5 0 0 1 9.9-1" />
    </>
  ),
  'circle-alert': (
    <>
      <circle cx="12" cy="12" r="10" />
      <line x1="12" x2="12" y1="8" y2="12" />
      <line x1="12" x2="12.01" y1="16" y2="16" />
    </>
  ),
  plus: (
    <>
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </>
  ),
  // 22 D2.2: the delete of a profile, in the status row of its page. The path
  // data is lucide-static icons/minus.svg: the plus's horizontal stroke, with
  // no vertical one.
  minus: <path d="M5 12h14" />,
  x: (
    <>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </>
  ),
  check: <path d="M20 6 9 17l-5-5" />,
  'arrow-up': (
    <>
      <path d="m5 12 7-7 7 7" />
      <path d="M12 19V5" />
    </>
  ),
  'arrow-down': (
    <>
      <path d="M12 5v14" />
      <path d="m19 12-7 7-7-7" />
    </>
  ),
  'arrow-up-down': (
    <>
      <path d="m21 16-4 4-4-4" />
      <path d="M17 20V4" />
      <path d="m3 8 4-4 4 4" />
      <path d="M7 4v16" />
    </>
  ),
}

/** The icon of `name`, to be the first child of the element that holds it. */
export function Icon({ name }: { name: IconName }) {
  return (
    <svg
      className="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      {SHAPES[name]}
    </svg>
  )
}

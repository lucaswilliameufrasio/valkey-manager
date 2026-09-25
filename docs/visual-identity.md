# Visual identity

Valkey Manager uses a calm, high-contrast dark workspace tuned for long operational
sessions. The existing V mark supplies the visual anchor: a mint-to-lime gradient,
warm amber data points, and deep blue-green surfaces.

## Design tokens

| Role | Color | Use |
| --- | --- | --- |
| Canvas | `#0A171C` | Main application background |
| Surface | `#102228` | Sidebar, cards, and navigation |
| Raised surface | `#172E34` | Inputs, rows, and secondary controls |
| Primary mint | `#36DEC5` | Selection, interactive focus, and connection accent |
| Valkey lime | `#B9EF61` | Healthy/connected state and positive metrics |
| Signal amber | `#FFC85A` | Attention and secondary metric accents |
| Primary text | `#F2FAF7` | Headings and operational values |
| Muted text | `#97AEAC` | Supporting information and labels |

The interface uses eframe's bundled proportional font for product copy and monospace
for endpoints, key names, and console data. Cards and controls use restrained rounded
corners, thin blue-green borders, consistent spacing, and color only where it conveys
state or hierarchy. Destructive actions remain visually distinct from routine tasks.

The palette and reusable widgets live in `src/visual.rs`; the UI applies the shared
theme once at startup so future screens inherit the same tokens.

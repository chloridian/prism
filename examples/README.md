# Examples

Every example is a target of the `examples` package. Run one from the repository root:

```sh
cargo run --release --example <name>
```

The component gallery is a separate crate:

```sh
cargo run --release -p gpui-component-story
```

## gpui/basics

- `hello_world`: a window with text and colored boxes.
- `interactive_elements`: hover, click and focus states on plain elements.
- `actions_and_keybinds`: defining actions and binding keys to them.
- `uniform_list`: a virtualized list of equal-height rows.
- `creating_components`: building reusable components with `RenderOnce`.
- `async_tasks`: spawning foreground and background tasks.
- `tab_stop`: tab order across focusable elements.
- `focus_visible`: showing focus rings for keyboard focus only.
- `tree`: a collapsible tree.
- `scrollable`: scrolling containers.
- `switch`: the switch control from `gpui_elements`.

## gpui/layout_and_style

- `layout`: flexbox layout.
- `styling`: colors, borders, radii and shadows.
- `direction`: left-to-right and right-to-left layout.
- `inline_layout`: text and boxes flowing in one paragraph.
- `border_and_shadow_backgrounds`: backgrounds drawn under borders and shadows.
- `gradient`: linear gradients.
- `opacity`: opacity on elements, images and SVGs.
- `corner_smoothing`: continuous corner curves.
- `blur`: background blur.

## gpui/animation

- `animation`: rotation, bounce and scale animations.
- `transitions`: transitions between element states.
- `style_transitions`: animated style changes.
- `motion_showcase`: springs and easing curves side by side.

## gpui/text

- `text`: text styling, wrapping and alignment.
- `text_system`: shaping and measuring text directly.
- `text_transform_preview`: uppercase, lowercase and capitalize.
- `editable_text`: the editable text element from `gpui_elements`.

## gpui/windows

- `window`: opening windows with different options.
- `window_positioning`: placing a window on a display.
- `window_movable`: dragging a window by its content.
- `window_visibility`: showing and hiding a window.
- `window_shadow`: client-side window decorations and shadows.
- `on_window_close_quit`: quitting when the last window closes.

## gpui/graphics

- `custom_drawing`: painting paths and quads in an element.
- `custom_gpu`: rendering with a custom wgpu pipeline.
- `svg`: SVG elements tinted with the text color.
- `image_loading`: image sources, loading and fallback states.
- `image`: local, remote and asset images, including EXIF orientation.
- `gif_viewer`: animated GIFs.

## gpui/platform

- `keyring`: storing credentials in the system keyring.
- `system_notifications`: posting system notifications.

## gpui/stress

- `data_table`: a 10,000-row table with virtualized scrolling.
- `paths_bench`: many vector paths per frame.
- `pattern`: pattern fills.
- `shadow`: a grid of shadow sizes and blurs.

## kit

- `input`: a single-line text input.
- `editor`: a code editor with syntax highlighting.
- `dock`: a dock area with draggable panels.
- `markdown`: a Markdown editor with a live preview.
- `html`: an HTML editor with a live preview.
- `sidebar`: a collapsible sidebar.
- `text_selection`: selecting text across elements.
- `window_title`: one window with the story's title bar and controls for its height, disabled minimize and maximize buttons, and a close-only bar.
- `table_in_scrollable`: a table inside a scrolling container.

Fixtures for the kit examples live in `kit/fixtures`. Assets shared by the gpui examples live in `gpui/assets`.

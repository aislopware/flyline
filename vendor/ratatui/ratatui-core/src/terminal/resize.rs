use crate::backend::{Backend, ClearType};
use crate::layout::Rect;
use crate::terminal::{Terminal, Viewport};

impl<B: Backend> Terminal<B> {
    /// Updates the Terminal so that internal buffers match the requested area.
    ///
    /// This updates the buffer size used for rendering and clears the affected viewport so the
    /// next [`Terminal::draw`] / [`Terminal::try_draw`] paints into a consistent area.
    ///
    /// When the viewport is [`Viewport::Inline`], the `area` argument is treated as the new
    /// terminal size and the viewport origin is recomputed relative to the current cursor position.
    /// Ratatui attempts to keep the cursor at the same relative row within the viewport across
    /// resizes.
    ///
    /// See also: [`Terminal::autoresize`] (automatic resizing during [`Terminal::draw`] /
    /// [`Terminal::try_draw`]).
    ///
    /// For [`Viewport::Fixed`] and [`Viewport::Fullscreen`], `area` becomes the new viewport area.
    /// For [`Viewport::Inline`], `area` is interpreted as the backend's new terminal size and the
    /// viewport origin may move to preserve the cursor's relative row within the inline UI.
    pub fn resize(&mut self, area: Rect) -> Result<(), B::Error> {
        if matches!(self.viewport, Viewport::Inline(_)) {
            let old_width = self.viewport_area.width;
            let new_width = area.width;
            let old_height = self.last_known_area.height;
            let new_height = area.height;

            if (old_width > 0 && old_width != new_width)
                || (old_height > 0 && old_height != new_height)
            {
                self.viewport_top = None;
            }
            let height = match self.viewport {
                Viewport::Inline(h) => area.height.min(h),
                _ => unreachable!(),
            };
            let new_width = area.width;

            // Disabled cursor relocation and line-wrapping calculation on resize.
            // Leaving the cursor where it is.

            // self.inline_cursor_x = 0;
            // self.inline_cursor_y = 0;
            self.set_viewport_area(Rect {
                x: 0,
                y: 0,
                width: new_width,
                height,
            });
            self.buffers[0].reset();
            self.buffers[1].reset();
            self.force_full_redraw = true;
            self.last_known_area = area;
            return Ok(());
        }

        let mut next_area = area;
        // Clear the screen when a fixed viewport shrinks horizontally, to avoid line wrapping
        // issues. A fullscreen viewport's `clear_viewport` below already erases every row.
        if next_area.width < self.viewport_area.width {
            next_area.y = 0;
            if matches!(self.viewport, Viewport::Fixed(_)) {
                self.backend.clear_region(ClearType::All)?;
            }
        }

        self.set_viewport_area(next_area);
        self.clear_viewport()?;
        self.last_known_area = area;
        Ok(())
    }

    /// Queries the backend for size and resizes if it doesn't match the previous size.
    ///
    /// This is called automatically during [`Terminal::draw`] / [`Terminal::try_draw`] for
    /// fullscreen and inline viewports. Fixed viewports are not automatically resized.
    ///
    /// If the size changed, this calls [`Terminal::resize`] and therefore clears the affected
    /// region before the next frame is rendered.
    pub fn autoresize(&mut self) -> Result<(), B::Error> {
        // fixed viewports do not get autoresized
        if matches!(self.viewport, Viewport::Fullscreen | Viewport::Inline(_)) {
            let area = self.size()?.into();
            if area != self.last_known_area {
                self.resize(area)?;
            }
        }
        Ok(())
    }

    /// Resize internal buffers and update the current viewport area.
    ///
    /// This is an internal helper used by [`Terminal::with_options`] and [`Terminal::resize`].
    pub(crate) fn set_viewport_area(&mut self, area: Rect) {
        self.buffers[self.current].resize(area);
        self.buffers[1 - self.current].resize(area);
        self.viewport_area = area;
        self.update_viewport_top_for_height();
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use rstest::rstest;

    use crate::backend::{Backend, ClearType, TestBackend, WindowSize};
    use crate::buffer::Buffer;
    use crate::layout::{Position, Rect, Size};
    use crate::terminal::{Terminal, TerminalOptions, Viewport};

    #[derive(Debug, Default)]
    struct ClearBackend(Vec<ClearType>);

    impl Backend for ClearBackend {
        type Error = core::convert::Infallible;

        fn draw<'a, I>(&mut self, _content: I) -> Result<(), Self::Error>
        where
            I: Iterator<Item = (u16, u16, &'a crate::buffer::Cell)>,
        {
            Ok(())
        }

        fn hide_cursor(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn show_cursor(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
            Ok(Position::ORIGIN)
        }

        fn set_cursor_position<P: Into<Position>>(
            &mut self,
            _position: P,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        fn clear(&mut self) -> Result<(), Self::Error> {
            self.clear_region(ClearType::All)
        }

        fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
            self.0.push(clear_type);
            Ok(())
        }

        fn size(&self) -> Result<Size, Self::Error> {
            Ok(Size::new(80, 24))
        }

        fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
            Ok(WindowSize {
                columns_rows: self.size()?,
                pixels: Size::default(),
            })
        }

        fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn clears_on_horizontal_shrink(viewport: Viewport) -> Vec<ClearType> {
        let backend = ClearBackend::default();
        let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
        terminal.backend_mut().0.clear();
        terminal.resize(Rect::new(0, 0, 40, 24)).unwrap();
        terminal.backend().0.clone()
    }

    #[rstest]
    #[case::fullscreen(Viewport::Fullscreen, &[ClearType::All])]
    // Resizing an inline viewport clears nothing: it resets the buffers and forces a full
    // redraw, which erases stale cells as it repaints.
    #[case::inline(Viewport::Inline(5), &[])]
    #[case::fixed(
        Viewport::Fixed(Rect::new(0, 0, 80, 24)),
        &[ClearType::All]
    )]
    fn resize_horizontal_shrink_clears_expected_regions(
        #[case] viewport: Viewport,
        #[case] expected: &[ClearType],
    ) {
        assert_eq!(clears_on_horizontal_shrink(viewport), expected);
    }

    #[test]
    fn resize_fullscreen_updates_viewport_and_buffer_areas() {
        let backend = TestBackend::new(3, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.backend_mut().resize(4, 3);
        let new_area = Rect::new(0, 0, 4, 3);
        terminal.resize(new_area).unwrap();

        assert_eq!(terminal.viewport_area, new_area);
        assert_eq!(terminal.last_known_area, new_area);
        assert_eq!(terminal.buffers[terminal.current].area, new_area);
        assert_eq!(terminal.buffers[1 - terminal.current].area, new_area);
    }

    #[test]
    fn resize_fullscreen_triggers_clear_and_resets_back_buffer() {
        // This test is specifically about the side effects of `resize`:
        // - it calls `clear` to force a full redraw
        // - it resets the "previous" buffer
        let backend = TestBackend::new(3, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        // Put visible content on the backend so we can tell whether a clear happened.
        {
            let frame = terminal.get_frame();
            frame.buffer[(0, 0)].set_symbol("x");
        }
        terminal.flush().unwrap();
        terminal.backend().assert_buffer_lines(["x  ", "   "]);

        terminal.backend_mut().resize(4, 3);
        let new_area = Rect::new(0, 0, 4, 3);
        terminal.resize(new_area).unwrap();

        terminal
            .backend()
            .assert_buffer_lines(["    ", "    ", "    "]);
        assert_eq!(
            terminal.buffers[1 - terminal.current],
            Buffer::empty(new_area)
        );
    }

    #[test]
    fn autoresize_fullscreen_uses_backend_size_when_changed() {
        let backend = TestBackend::new(3, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        {
            let frame = terminal.get_frame();
            frame.buffer[(0, 0)].set_symbol("x");
        }
        terminal.flush().unwrap();

        terminal.backend_mut().resize(4, 3);
        terminal.autoresize().unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 4, 3));
        assert_eq!(terminal.last_known_area, Rect::new(0, 0, 4, 3));
        terminal
            .backend()
            .assert_buffer_lines(["    ", "    ", "    "]);
    }

    #[test]
    fn autoresize_fixed_does_not_change_viewport() {
        let backend = TestBackend::with_lines(["xxx", "yyy"]);
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(1, 0, 2, 2)),
            },
        )
        .unwrap();

        terminal.autoresize().unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(1, 0, 2, 2));
        assert_eq!(terminal.last_known_area, Rect::new(1, 0, 2, 2));
        terminal.backend().assert_buffer_lines(["xxx", "yyy"]);
    }

    #[test]
    fn resize_fixed_changes_viewport_area_and_buffer_sizes() {
        let backend = TestBackend::new(5, 3);
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Fixed(Rect::new(1, 1, 2, 1)),
            },
        )
        .unwrap();

        terminal.resize(Rect::new(0, 0, 3, 2)).unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 3, 2));
        assert_eq!(terminal.last_known_area, Rect::new(0, 0, 3, 2));
        assert_eq!(
            terminal.buffers[terminal.current].area,
            terminal.viewport_area
        );
        assert_eq!(
            terminal.buffers[1 - terminal.current].area,
            terminal.viewport_area
        );
    }

    #[test]
    fn resize_inline_recomputes_origin_using_previous_cursor_offset() {
        let mut backend = TestBackend::new(10, 10);
        backend
            .set_cursor_position(Position { x: 0, y: 4 })
            .unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(4),
            },
        )
        .unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 10, 4));

        terminal.last_known_cursor_pos = Position { x: 0, y: 5 };
        terminal.backend_mut().resize(10, 12);
        let new_terminal_area = Rect::new(0, 0, 10, 12);
        terminal.resize(new_terminal_area).unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 10, 4));
        assert_eq!(terminal.last_known_area, new_terminal_area);
    }

    #[test]
    fn resize_inline_clamps_height_to_terminal_height() {
        let mut backend = TestBackend::new(10, 10);
        backend
            .set_cursor_position(Position { x: 0, y: 0 })
            .unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(10),
            },
        )
        .unwrap();

        terminal.backend_mut().resize(10, 3);
        terminal.resize(Rect::new(0, 0, 10, 3)).unwrap();

        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 10, 3));
    }

    #[test]
    fn resize_inline_preserves_backend_cursor_across_repeated_resizes() {
        let mut backend = TestBackend::new(10, 10);
        backend
            .set_cursor_position(Position { x: 0, y: 4 })
            .unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(4),
            },
        )
        .unwrap();

        terminal.resize(Rect::new(0, 0, 10, 12)).unwrap();
        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 10, 4));
        terminal.resize(Rect::new(0, 0, 10, 14)).unwrap();
        assert_eq!(terminal.viewport_area, Rect::new(0, 0, 10, 4));
        assert_eq!(
            terminal.backend().cursor_position(),
            Position { x: 0, y: 4 }
        );
    }

    // This tests for the case where the new width is smaller than the old
    // width. The internal buffers are reset so the next draw repaints the
    // viewport into a consistent area.
    #[test]
    fn resize_inline_resets_the_buffers_on_horizontal_shrink() {
        let mut backend = TestBackend::with_lines(["0000", "1111"]);
        backend
            .set_cursor_position(Position { x: 0, y: 0 })
            .unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(2),
            },
        )
        .unwrap();

        let old_area = terminal.backend().buffer().area;
        let new_area = Rect {
            width: old_area.width - 1,
            ..old_area
        };

        terminal.resize(new_area).unwrap();
        assert_eq!(terminal.viewport_area, new_area);
        let all_clear = terminal
            .current_buffer_mut()
            .content()
            .iter()
            .all(|cell| cell == &crate::buffer::Cell::EMPTY);
        assert!(all_clear, "not all buffer cells are empty");
    }

    #[test]
    fn resize_inline_bounds_cursor_movement_to_viewport_lines() {
        let mut backend = TestBackend::with_lines(["12345678", "12345678"]);
        backend
            .set_cursor_position(Position { x: 4, y: 1 })
            .unwrap();
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(2),
            },
        )
        .unwrap();

        terminal.inline_cursor_x = 4;
        terminal.inline_cursor_y = 1;

        // Shrink width from 8 to 4. Line 0 (8 chars) wraps into 2 rows (extra_rows = 1).
        // total_rows_up = inline_cursor_y (1) + extra_rows (1) + cursor_wrap (0) = 2.
        terminal.resize(Rect::new(0, 0, 4, 10)).unwrap();

        // Resizing leaves the cursor where it is; the application repositions it.
        assert_eq!(terminal.inline_cursor_y, 1);
        assert_eq!(terminal.inline_cursor_x, 4);
        assert_eq!(terminal.viewport_area.width, 4);
        assert!(terminal.force_full_redraw);
    }
}

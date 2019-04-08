use termion::event::Event;

use crate::coordinates::{Coordinates, Position, Size};
use crate::fail::{ErrorLog, HError, HResult};
use crate::widget::{Widget, WidgetCore};

#[derive(PartialEq)]
pub struct HBox<T: Widget> {
    pub core: WidgetCore,
    pub widgets: Vec<T>,
    pub ratios: Option<Vec<usize>>,
    pub zoom_active: bool,
    pub active: Option<usize>,
}

impl<T> HBox<T>
where
    T: Widget + PartialEq,
{
    pub fn new(core: &WidgetCore) -> HBox<T> {
        HBox {
            core: core.clone(),
            widgets: vec![],
            ratios: None,
            zoom_active: false,
            active: None,
        }
    }

    pub fn resize_children(&mut self) -> HResult<()> {
        let len = self.widgets.len();
        if len == 0 {
            return Ok(());
        }

        if self.zoom_active {
            let coords = self.core.coordinates.clone();
            self.active_widget_mut()?.set_coordinates(&coords).log();
            return Ok(());
        }

        let coords: Vec<Coordinates> = self.calculate_coordinates()?;

        for (widget, coord) in self.widgets.iter_mut().zip(coords.iter()) {
            widget.set_coordinates(coord).log();
        }

        Ok(())
    }

    pub fn push_widget(&mut self, widget: T) {
        self.widgets.push(widget);
    }

    pub fn pop_widget(&mut self) -> Option<T> {
        let widget = self.widgets.pop();
        widget
    }

    pub fn remove_widget(&mut self, index: usize) -> T {
        self.widgets.remove(index)
    }

    pub fn prepend_widget(&mut self, widget: T) {
        self.widgets.insert(0, widget);
    }

    pub fn insert_widget(&mut self, index: usize, widget: T) {
        self.widgets.insert(index, widget);
    }

    pub fn replace_widget(&mut self, index: usize, mut widget: T) -> T {
        std::mem::swap(&mut self.widgets[index], &mut widget);
        widget
    }

    pub fn toggle_zoom(&mut self) -> HResult<()> {
        self.clear().log();
        self.zoom_active = !self.zoom_active;
        self.resize_children()
    }

    pub fn set_ratios(&mut self, ratios: Vec<usize>) {
        self.ratios = Some(ratios);
    }

    pub fn calculate_equal_ratios(&self) -> HResult<Vec<usize>> {
        let len = self.widgets.len();
        if len == 0 {
            return HError::no_widget();
        }

        let ratios = (0..len).map(|_| 100 / len).collect();
        Ok(ratios)
    }

    pub fn calculate_coordinates(&self) -> HResult<Vec<Coordinates>> {
        let box_coords = self.get_coordinates()?;
        let box_xsize = box_coords.xsize();
        let box_ysize = box_coords.ysize();
        let box_top = box_coords.top().y();

        let ratios = match &self.ratios {
            Some(ratios) => ratios.clone(),
            None => self.calculate_equal_ratios()?,
        };

        let coords = ratios
            .iter()
            .fold(Vec::<Coordinates>::new(), |mut coords, ratio| {
                let ratio = *ratio as u16;
                let len = coords.len();
                let gap = if len == 0 { 0 } else { 1 };

                let widget_xsize = box_xsize * ratio / 100;
                let widget_xpos = if len == 0 {
                    box_coords.top().x()
                } else {
                    let prev_coords = coords.last().unwrap();
                    let prev_xsize = prev_coords.xsize();
                    let prev_xpos = prev_coords.position().x();

                    prev_xsize + prev_xpos + gap
                };

                coords.push(Coordinates {
                    size: Size((widget_xsize, box_ysize)),
                    position: Position((widget_xpos, box_top)),
                });
                coords
            });

        Ok(coords)
    }

    pub fn set_active(&mut self, i: usize) -> HResult<()> {
        if i + 1 > self.widgets.len() {
            HError::no_widget()?
        }
        self.active = Some(i);
        Ok(())
    }

    pub fn active_widget(&self) -> Option<&T> {
        self.widgets.get(self.active?)
    }

    pub fn active_widget_mut(&mut self) -> Option<&mut T> {
        self.widgets.get_mut(self.active?)
    }
}

impl<T> Widget for HBox<T>
where
    T: Widget + PartialEq,
{
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }

    fn set_coordinates(&mut self, coordinates: &Coordinates) -> HResult<()> {
        self.core.coordinates = coordinates.clone();
        self.resize_children()
    }

    fn render_header(&self) -> HResult<String> {
        self.active_widget()?.render_header()
    }

    fn refresh(&mut self) -> HResult<()> {
        if self.zoom_active {
            self.active_widget_mut()?.refresh().log();
            return Ok(());
        }

        self.resize_children().log();
        for child in &mut self.widgets {
            child.refresh().log();
        }
        Ok(())
    }

    fn get_drawlist(&self) -> HResult<String> {
        if self.zoom_active {
            return self.active_widget()?.get_drawlist();
        }

        Ok(self
            .widgets
            .iter()
            .map(|child| {
                child
                    .get_drawlist()
                    .log_and()
                    .unwrap_or_else(|_| String::new())
            })
            .collect())
    }

    fn on_event(&mut self, event: Event) -> HResult<()> {
        self.active_widget_mut()?.on_event(event)?;
        Ok(())
    }
}

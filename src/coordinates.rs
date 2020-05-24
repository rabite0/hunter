use crate::fail::HResult;

#[derive(Debug, Clone, PartialEq)]
pub struct Size(pub (u16, u16));
#[derive(Debug, Clone, PartialEq)]
pub struct Position(pub (u16, u16));

#[derive(Debug, Clone, PartialEq)]
pub struct Coordinates {
    pub size: Size,
    pub position: Position,
}

impl Coordinates {
    pub fn new() -> Coordinates {
        Coordinates {
            size: Size((crate::term::xsize(), crate::term::ysize())),
            position: Position((1, 1)),
        }
    }

    pub fn new_at(xsize: u16, ysize: u16, xpos: u16, ypos: u16) -> Coordinates {
        Coordinates {
            size: Size((xsize, ysize)),
            position: Position((xpos, ypos)),
        }
    }

    // pub fn size(&self) -> &Size {
    //      &self.size
    // }

    pub fn set_size(&mut self, x: u16, y: u16) {
        self.size.0 = (x, y);
    }

    pub fn set_size_u(&mut self, x: usize, y: usize) {
        self.size.0 = ((x + 1) as u16, (y + 1) as u16);
    }

    pub fn set_xsize(&mut self, x: u16) {
        (self.size.0).0 = x;
    }

    pub fn set_ysize(&mut self, y: u16) {
        (self.size.0).1 = y;
    }

    pub fn set_position(&mut self, x: u16, y: u16) {
        self.position.0 = (x, y);
    }

    pub fn set_position_u(&mut self, x: usize, y: usize) {
        self.position.0 = ((x + 1) as u16, (y + 1) as u16);
    }

    pub fn set_xpos(&mut self, x: u16) {
        (self.position.0).0 = x;
    }

    pub fn set_ypos(&mut self, y: u16) {
        (self.position.0).1 = y;
    }

    pub fn xsize_u(&self) -> usize {
        self.size.size_u().0
    }

    pub fn xsize(&self) -> u16 {
        self.size.xsize()
    }

    pub fn ysize_u(&self) -> usize {
        (self.ysize() - 1) as usize
    }

    pub fn ysize(&self) -> u16 {
        self.size.ysize()
    }

    pub fn xpos(&self) -> u16 {
        self.position.position().0
    }

    pub fn ypos(&self) -> u16 {
        self.position.position().1
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn u16position(&self) -> (u16, u16) {
        self.position.position()
    }

    pub fn position_u(&self) -> (usize, usize) {
        let (xpos, ypos) = self.u16position();
        ((xpos - 1) as usize, (ypos - 1) as usize)
    }

    pub fn size(&self) -> &Size {
        &self.size
    }

    pub fn u16size(&self) -> (u16, u16) {
        self.size.size()
    }

    pub fn size_u(&self) -> (usize, usize) {
        let (xsize, ysize) = self.u16size();
        ((xsize - 1) as usize, (ysize - 1) as usize)
    }

    pub fn size_pixels(&self) -> HResult<(usize, usize)> {
        let (xsize, ysize) = self.size_u();
        let (cols, rows) = crate::term::size()?;
        let (xpix, ypix) = crate::term::size_pixels()?;
        // Cell dimensions
        let (xpix, ypix) = (xpix / cols, ypix / rows);
        // Frame dimensions
        let (xpix, ypix) = (xpix * (xsize + 1), ypix * (ysize + 1));

        Ok((xpix as usize, ypix as usize))
    }

    pub fn top(&self) -> Position {
        self.position().clone()
    }

    //    pub fn left(&self) -> /
}

impl Size {
    pub fn size(&self) -> (u16, u16) {
        self.0
    }
    pub fn size_u(&self) -> (usize, usize) {
        let (xsize, ysize) = self.0;
        ((xsize - 1) as usize, (ysize - 1) as usize)
    }
    pub fn xsize(&self) -> u16 {
        (self.0).0
    }
    pub fn ysize(&self) -> u16 {
        (self.0).1
    }
}

impl Position {
    pub fn position(&self) -> (u16, u16) {
        self.0
    }
    pub fn position_u(&self) -> (usize, usize) {
        let (xpos, ypos) = self.0;
        ((xpos - 1) as usize, (ypos - 1) as usize)
    }
    pub fn x(&self) -> u16 {
        (self.0).0
    }
    pub fn y(&self) -> u16 {
        (self.0).1
    }
}

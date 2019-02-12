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

    pub fn new_at(xsize: u16, ysize: u16, xpos: u16, ypos: u16 ) -> Coordinates {
        Coordinates {
            size: Size((xsize, ysize)),
            position: Position((xpos, ypos))
        }
    }

    // pub fn size(&self) -> &Size {
    //      &self.size
    // }

    pub fn xsize(&self) -> u16 {
        self.size.xsize()
    }

    pub fn ysize(&self) -> u16 {
        self.size.ysize()
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn u16position(&self) -> (u16, u16) {
        self.position.position()
    }

    pub fn size(&self) -> &Size {
        &self.size
    }

    pub fn u16size(&self) -> (u16, u16) {
        self.size.size()
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
    pub fn x(&self) -> u16 {
        (self.0).0
    }
    pub fn y(&self) -> u16 {
        (self.0).1
    }
}

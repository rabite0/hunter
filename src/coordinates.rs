#[derive(Debug,Clone)]
pub struct Size(pub (u16,u16));
#[derive(Debug,Clone)]
pub struct Position(pub (u16,u16));


#[derive(Debug,Clone)]
pub struct Coordinates {
    pub size: Size,
    pub position: Position,
}

impl Coordinates {
    pub fn size(&self) -> &Size {
        &self.size
    }

    pub fn xsize(&self) -> u16 {
        self.size.xsize()
    }

    pub fn ysize(&self) -> u16 {
        self.size.ysize()
    }

    pub fn position(&self) -> &Position {
        &self.position
    }

    pub fn top(&self) -> Position {
        self.position().clone()
    }

//    pub fn left(&self) -> /
}

impl Size {
    pub fn size(&self) -> (u16,u16) {
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
    pub fn position(&self) -> (u16,u16) {
        self.0
    }
    pub fn x(&self) -> u16 {
        (self.0).1
    }
    pub fn y(&self) -> u16 {
        (self.0).1
    }
}

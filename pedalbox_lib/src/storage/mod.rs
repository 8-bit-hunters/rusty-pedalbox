pub trait Storage<T> {
    type Error;

    fn load(&mut self) -> Option<T>;
    fn save(&mut self, value: &T) -> Result<(), Self::Error>;
}

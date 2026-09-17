import SwiftUI

struct LibraryView: View {
    @State private var selection: Set<Photo.ID> = []

    var body: some View {
        Menu("File") {
            Button("Open") { openPicker() }
            Button("Save changes") { save() }
        }
        .confirmationDialog(
            String(localized: "Delete \(selection.count) photos?"),
            isPresented: $confirmDelete
        ) { }
    }
}

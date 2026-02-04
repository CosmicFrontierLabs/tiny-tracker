// @generated automatically by Diesel CLI.

diesel::table! {
    action_items (id) {
        #[max_length = 20]
        id -> Varchar,
        vendor_id -> Int4,
        number -> Int4,
        #[max_length = 500]
        title -> Varchar,
        create_date -> Date,
        created_by_id -> Int4,
        due_date -> Nullable<Date>,
        owner_id -> Int4,
        #[max_length = 20]
        priority -> Varchar,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        description -> Nullable<Text>,
        category_id -> Int4,
    }
}

diesel::table! {
    categories (id) {
        id -> Int4,
        vendor_id -> Int4,
        #[max_length = 100]
        name -> Varchar,
        description -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    notes (id) {
        id -> Int4,
        #[max_length = 20]
        action_item_id -> Varchar,
        note_date -> Date,
        author_id -> Int4,
        content -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    status_history (id) {
        id -> Int4,
        #[max_length = 20]
        action_item_id -> Varchar,
        #[max_length = 50]
        status -> Varchar,
        changed_by_id -> Int4,
        changed_at -> Timestamptz,
        comment -> Nullable<Text>,
    }
}

diesel::table! {
    users (id) {
        id -> Int4,
        #[max_length = 255]
        email -> Varchar,
        #[max_length = 255]
        name -> Varchar,
        #[max_length = 10]
        initials -> Nullable<Varchar>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    vendors (id) {
        id -> Int4,
        #[max_length = 10]
        prefix -> Varchar,
        #[max_length = 255]
        name -> Varchar,
        description -> Nullable<Text>,
        next_number -> Int4,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(action_items -> categories (category_id));
diesel::joinable!(action_items -> vendors (vendor_id));
diesel::joinable!(categories -> vendors (vendor_id));
diesel::joinable!(notes -> action_items (action_item_id));
diesel::joinable!(notes -> users (author_id));
diesel::joinable!(status_history -> action_items (action_item_id));
diesel::joinable!(status_history -> users (changed_by_id));

diesel::allow_tables_to_appear_in_same_query!(
    action_items,
    categories,
    notes,
    status_history,
    users,
    vendors,
);

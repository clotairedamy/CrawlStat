// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint, type=warning, deprecated_member_use, deprecated_member_use_from_same_package
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'types.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
T _$identity<T>(T value) => value;
/// @nodoc
mixin _$CrawlEvent {





@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'CrawlEvent()';
}


}

/// @nodoc
class $CrawlEventCopyWith<$Res>  {
$CrawlEventCopyWith(CrawlEvent _, $Res Function(CrawlEvent) __);
}


/// Adds pattern-matching-related methods to [CrawlEvent].
extension CrawlEventPatterns on CrawlEvent {
/// A variant of `map` that fallback to returning `orElse`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeMap<TResult extends Object?>({TResult Function( CrawlEvent_Started value)?  started,TResult Function( CrawlEvent_Url value)?  url,TResult Function( CrawlEvent_Progress value)?  progress,TResult Function( CrawlEvent_Warning value)?  warning,TResult Function( CrawlEvent_StateChanged value)?  stateChanged,TResult Function( CrawlEvent_AuditStarted value)?  auditStarted,TResult Function( CrawlEvent_Audited value)?  audited,TResult Function( CrawlEvent_Finished value)?  finished,TResult Function( CrawlEvent_Failed value)?  failed,required TResult orElse(),}){
final _that = this;
switch (_that) {
case CrawlEvent_Started() when started != null:
return started(_that);case CrawlEvent_Url() when url != null:
return url(_that);case CrawlEvent_Progress() when progress != null:
return progress(_that);case CrawlEvent_Warning() when warning != null:
return warning(_that);case CrawlEvent_StateChanged() when stateChanged != null:
return stateChanged(_that);case CrawlEvent_AuditStarted() when auditStarted != null:
return auditStarted(_that);case CrawlEvent_Audited() when audited != null:
return audited(_that);case CrawlEvent_Finished() when finished != null:
return finished(_that);case CrawlEvent_Failed() when failed != null:
return failed(_that);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// Callbacks receives the raw object, upcasted.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case final Subclass2 value:
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult map<TResult extends Object?>({required TResult Function( CrawlEvent_Started value)  started,required TResult Function( CrawlEvent_Url value)  url,required TResult Function( CrawlEvent_Progress value)  progress,required TResult Function( CrawlEvent_Warning value)  warning,required TResult Function( CrawlEvent_StateChanged value)  stateChanged,required TResult Function( CrawlEvent_AuditStarted value)  auditStarted,required TResult Function( CrawlEvent_Audited value)  audited,required TResult Function( CrawlEvent_Finished value)  finished,required TResult Function( CrawlEvent_Failed value)  failed,}){
final _that = this;
switch (_that) {
case CrawlEvent_Started():
return started(_that);case CrawlEvent_Url():
return url(_that);case CrawlEvent_Progress():
return progress(_that);case CrawlEvent_Warning():
return warning(_that);case CrawlEvent_StateChanged():
return stateChanged(_that);case CrawlEvent_AuditStarted():
return auditStarted(_that);case CrawlEvent_Audited():
return audited(_that);case CrawlEvent_Finished():
return finished(_that);case CrawlEvent_Failed():
return failed(_that);}
}
/// A variant of `map` that fallback to returning `null`.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case final Subclass value:
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? mapOrNull<TResult extends Object?>({TResult? Function( CrawlEvent_Started value)?  started,TResult? Function( CrawlEvent_Url value)?  url,TResult? Function( CrawlEvent_Progress value)?  progress,TResult? Function( CrawlEvent_Warning value)?  warning,TResult? Function( CrawlEvent_StateChanged value)?  stateChanged,TResult? Function( CrawlEvent_AuditStarted value)?  auditStarted,TResult? Function( CrawlEvent_Audited value)?  audited,TResult? Function( CrawlEvent_Finished value)?  finished,TResult? Function( CrawlEvent_Failed value)?  failed,}){
final _that = this;
switch (_that) {
case CrawlEvent_Started() when started != null:
return started(_that);case CrawlEvent_Url() when url != null:
return url(_that);case CrawlEvent_Progress() when progress != null:
return progress(_that);case CrawlEvent_Warning() when warning != null:
return warning(_that);case CrawlEvent_StateChanged() when stateChanged != null:
return stateChanged(_that);case CrawlEvent_AuditStarted() when auditStarted != null:
return auditStarted(_that);case CrawlEvent_Audited() when audited != null:
return audited(_that);case CrawlEvent_Finished() when finished != null:
return finished(_that);case CrawlEvent_Failed() when failed != null:
return failed(_that);case _:
  return null;

}
}
/// A variant of `when` that fallback to an `orElse` callback.
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return orElse();
/// }
/// ```

@optionalTypeArgs TResult maybeWhen<TResult extends Object?>({TResult Function( PlatformInt64 seeded,  bool resumed)?  started,TResult Function( CrawledUrlEvent field0)?  url,TResult Function( ProgressSnapshot field0)?  progress,TResult Function( String message,  String? url)?  warning,TResult Function( String state)?  stateChanged,TResult Function()?  auditStarted,TResult Function( PlatformInt64 errors,  PlatformInt64 warnings,  PlatformInt64 notices,  PlatformInt64 findings)?  audited,TResult Function( CrawlSummary summary)?  finished,TResult Function( String message)?  failed,required TResult orElse(),}) {final _that = this;
switch (_that) {
case CrawlEvent_Started() when started != null:
return started(_that.seeded,_that.resumed);case CrawlEvent_Url() when url != null:
return url(_that.field0);case CrawlEvent_Progress() when progress != null:
return progress(_that.field0);case CrawlEvent_Warning() when warning != null:
return warning(_that.message,_that.url);case CrawlEvent_StateChanged() when stateChanged != null:
return stateChanged(_that.state);case CrawlEvent_AuditStarted() when auditStarted != null:
return auditStarted();case CrawlEvent_Audited() when audited != null:
return audited(_that.errors,_that.warnings,_that.notices,_that.findings);case CrawlEvent_Finished() when finished != null:
return finished(_that.summary);case CrawlEvent_Failed() when failed != null:
return failed(_that.message);case _:
  return orElse();

}
}
/// A `switch`-like method, using callbacks.
///
/// As opposed to `map`, this offers destructuring.
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case Subclass2(:final field2):
///     return ...;
/// }
/// ```

@optionalTypeArgs TResult when<TResult extends Object?>({required TResult Function( PlatformInt64 seeded,  bool resumed)  started,required TResult Function( CrawledUrlEvent field0)  url,required TResult Function( ProgressSnapshot field0)  progress,required TResult Function( String message,  String? url)  warning,required TResult Function( String state)  stateChanged,required TResult Function()  auditStarted,required TResult Function( PlatformInt64 errors,  PlatformInt64 warnings,  PlatformInt64 notices,  PlatformInt64 findings)  audited,required TResult Function( CrawlSummary summary)  finished,required TResult Function( String message)  failed,}) {final _that = this;
switch (_that) {
case CrawlEvent_Started():
return started(_that.seeded,_that.resumed);case CrawlEvent_Url():
return url(_that.field0);case CrawlEvent_Progress():
return progress(_that.field0);case CrawlEvent_Warning():
return warning(_that.message,_that.url);case CrawlEvent_StateChanged():
return stateChanged(_that.state);case CrawlEvent_AuditStarted():
return auditStarted();case CrawlEvent_Audited():
return audited(_that.errors,_that.warnings,_that.notices,_that.findings);case CrawlEvent_Finished():
return finished(_that.summary);case CrawlEvent_Failed():
return failed(_that.message);}
}
/// A variant of `when` that fallback to returning `null`
///
/// It is equivalent to doing:
/// ```dart
/// switch (sealedClass) {
///   case Subclass(:final field):
///     return ...;
///   case _:
///     return null;
/// }
/// ```

@optionalTypeArgs TResult? whenOrNull<TResult extends Object?>({TResult? Function( PlatformInt64 seeded,  bool resumed)?  started,TResult? Function( CrawledUrlEvent field0)?  url,TResult? Function( ProgressSnapshot field0)?  progress,TResult? Function( String message,  String? url)?  warning,TResult? Function( String state)?  stateChanged,TResult? Function()?  auditStarted,TResult? Function( PlatformInt64 errors,  PlatformInt64 warnings,  PlatformInt64 notices,  PlatformInt64 findings)?  audited,TResult? Function( CrawlSummary summary)?  finished,TResult? Function( String message)?  failed,}) {final _that = this;
switch (_that) {
case CrawlEvent_Started() when started != null:
return started(_that.seeded,_that.resumed);case CrawlEvent_Url() when url != null:
return url(_that.field0);case CrawlEvent_Progress() when progress != null:
return progress(_that.field0);case CrawlEvent_Warning() when warning != null:
return warning(_that.message,_that.url);case CrawlEvent_StateChanged() when stateChanged != null:
return stateChanged(_that.state);case CrawlEvent_AuditStarted() when auditStarted != null:
return auditStarted();case CrawlEvent_Audited() when audited != null:
return audited(_that.errors,_that.warnings,_that.notices,_that.findings);case CrawlEvent_Finished() when finished != null:
return finished(_that.summary);case CrawlEvent_Failed() when failed != null:
return failed(_that.message);case _:
  return null;

}
}

}

/// @nodoc


class CrawlEvent_Started extends CrawlEvent {
  const CrawlEvent_Started({required this.seeded, required this.resumed}): super._();
  

 final  PlatformInt64 seeded;
 final  bool resumed;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_StartedCopyWith<CrawlEvent_Started> get copyWith => _$CrawlEvent_StartedCopyWithImpl<CrawlEvent_Started>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Started&&(identical(other.seeded, seeded) || other.seeded == seeded)&&(identical(other.resumed, resumed) || other.resumed == resumed));
}


@override
int get hashCode {
    return Object.hash(runtimeType,seeded,resumed);
}

@override
String toString() {
    return 'CrawlEvent.started(seeded: $seeded, resumed: $resumed)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_StartedCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_StartedCopyWith(CrawlEvent_Started value, $Res Function(CrawlEvent_Started) _then) = _$CrawlEvent_StartedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 seeded, bool resumed
});




}
/// @nodoc
class _$CrawlEvent_StartedCopyWithImpl<$Res>
    implements $CrawlEvent_StartedCopyWith<$Res> {
  _$CrawlEvent_StartedCopyWithImpl(this._self, this._then);

  final CrawlEvent_Started _self;
  final $Res Function(CrawlEvent_Started) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? seeded = null,Object? resumed = null,}) {
  return _then(CrawlEvent_Started(
seeded: null == seeded ? _self.seeded : seeded // ignore: cast_nullable_to_non_nullable
as PlatformInt64,resumed: null == resumed ? _self.resumed : resumed // ignore: cast_nullable_to_non_nullable
as bool,
  ));
}


}

/// @nodoc


class CrawlEvent_Url extends CrawlEvent {
  const CrawlEvent_Url(this.field0): super._();
  

 final  CrawledUrlEvent field0;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_UrlCopyWith<CrawlEvent_Url> get copyWith => _$CrawlEvent_UrlCopyWithImpl<CrawlEvent_Url>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Url&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'CrawlEvent.url(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_UrlCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_UrlCopyWith(CrawlEvent_Url value, $Res Function(CrawlEvent_Url) _then) = _$CrawlEvent_UrlCopyWithImpl;
@useResult
$Res call({
 CrawledUrlEvent field0
});




}
/// @nodoc
class _$CrawlEvent_UrlCopyWithImpl<$Res>
    implements $CrawlEvent_UrlCopyWith<$Res> {
  _$CrawlEvent_UrlCopyWithImpl(this._self, this._then);

  final CrawlEvent_Url _self;
  final $Res Function(CrawlEvent_Url) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CrawlEvent_Url(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as CrawledUrlEvent,
  ));
}


}

/// @nodoc


class CrawlEvent_Progress extends CrawlEvent {
  const CrawlEvent_Progress(this.field0): super._();
  

 final  ProgressSnapshot field0;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_ProgressCopyWith<CrawlEvent_Progress> get copyWith => _$CrawlEvent_ProgressCopyWithImpl<CrawlEvent_Progress>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Progress&&(identical(other.field0, field0) || other.field0 == field0));
}


@override
int get hashCode {
    return Object.hash(runtimeType,field0);
}

@override
String toString() {
    return 'CrawlEvent.progress(field0: $field0)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_ProgressCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_ProgressCopyWith(CrawlEvent_Progress value, $Res Function(CrawlEvent_Progress) _then) = _$CrawlEvent_ProgressCopyWithImpl;
@useResult
$Res call({
 ProgressSnapshot field0
});




}
/// @nodoc
class _$CrawlEvent_ProgressCopyWithImpl<$Res>
    implements $CrawlEvent_ProgressCopyWith<$Res> {
  _$CrawlEvent_ProgressCopyWithImpl(this._self, this._then);

  final CrawlEvent_Progress _self;
  final $Res Function(CrawlEvent_Progress) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? field0 = null,}) {
  return _then(CrawlEvent_Progress(
null == field0 ? _self.field0 : field0 // ignore: cast_nullable_to_non_nullable
as ProgressSnapshot,
  ));
}


}

/// @nodoc


class CrawlEvent_Warning extends CrawlEvent {
  const CrawlEvent_Warning({required this.message, this.url}): super._();
  

 final  String message;
 final  String? url;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_WarningCopyWith<CrawlEvent_Warning> get copyWith => _$CrawlEvent_WarningCopyWithImpl<CrawlEvent_Warning>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Warning&&(identical(other.message, message) || other.message == message)&&(identical(other.url, url) || other.url == url));
}


@override
int get hashCode {
    return Object.hash(runtimeType,message,url);
}

@override
String toString() {
    return 'CrawlEvent.warning(message: $message, url: $url)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_WarningCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_WarningCopyWith(CrawlEvent_Warning value, $Res Function(CrawlEvent_Warning) _then) = _$CrawlEvent_WarningCopyWithImpl;
@useResult
$Res call({
 String message, String? url
});




}
/// @nodoc
class _$CrawlEvent_WarningCopyWithImpl<$Res>
    implements $CrawlEvent_WarningCopyWith<$Res> {
  _$CrawlEvent_WarningCopyWithImpl(this._self, this._then);

  final CrawlEvent_Warning _self;
  final $Res Function(CrawlEvent_Warning) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,Object? url = freezed,}) {
  return _then(CrawlEvent_Warning(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,url: freezed == url ? _self.url : url // ignore: cast_nullable_to_non_nullable
as String?,
  ));
}


}

/// @nodoc


class CrawlEvent_StateChanged extends CrawlEvent {
  const CrawlEvent_StateChanged({required this.state}): super._();
  

 final  String state;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_StateChangedCopyWith<CrawlEvent_StateChanged> get copyWith => _$CrawlEvent_StateChangedCopyWithImpl<CrawlEvent_StateChanged>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_StateChanged&&(identical(other.state, state) || other.state == state));
}


@override
int get hashCode {
    return Object.hash(runtimeType,state);
}

@override
String toString() {
    return 'CrawlEvent.stateChanged(state: $state)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_StateChangedCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_StateChangedCopyWith(CrawlEvent_StateChanged value, $Res Function(CrawlEvent_StateChanged) _then) = _$CrawlEvent_StateChangedCopyWithImpl;
@useResult
$Res call({
 String state
});




}
/// @nodoc
class _$CrawlEvent_StateChangedCopyWithImpl<$Res>
    implements $CrawlEvent_StateChangedCopyWith<$Res> {
  _$CrawlEvent_StateChangedCopyWithImpl(this._self, this._then);

  final CrawlEvent_StateChanged _self;
  final $Res Function(CrawlEvent_StateChanged) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? state = null,}) {
  return _then(CrawlEvent_StateChanged(
state: null == state ? _self.state : state // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

/// @nodoc


class CrawlEvent_AuditStarted extends CrawlEvent {
  const CrawlEvent_AuditStarted(): super._();
  






@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_AuditStarted);
}


@override
int get hashCode => runtimeType.hashCode;

@override
String toString() {
    return 'CrawlEvent.auditStarted()';
}


}




/// @nodoc


class CrawlEvent_Audited extends CrawlEvent {
  const CrawlEvent_Audited({required this.errors, required this.warnings, required this.notices, required this.findings}): super._();
  

 final  PlatformInt64 errors;
 final  PlatformInt64 warnings;
 final  PlatformInt64 notices;
 final  PlatformInt64 findings;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_AuditedCopyWith<CrawlEvent_Audited> get copyWith => _$CrawlEvent_AuditedCopyWithImpl<CrawlEvent_Audited>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Audited&&(identical(other.errors, errors) || other.errors == errors)&&(identical(other.warnings, warnings) || other.warnings == warnings)&&(identical(other.notices, notices) || other.notices == notices)&&(identical(other.findings, findings) || other.findings == findings));
}


@override
int get hashCode {
    return Object.hash(runtimeType,errors,warnings,notices,findings);
}

@override
String toString() {
    return 'CrawlEvent.audited(errors: $errors, warnings: $warnings, notices: $notices, findings: $findings)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_AuditedCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_AuditedCopyWith(CrawlEvent_Audited value, $Res Function(CrawlEvent_Audited) _then) = _$CrawlEvent_AuditedCopyWithImpl;
@useResult
$Res call({
 PlatformInt64 errors, PlatformInt64 warnings, PlatformInt64 notices, PlatformInt64 findings
});




}
/// @nodoc
class _$CrawlEvent_AuditedCopyWithImpl<$Res>
    implements $CrawlEvent_AuditedCopyWith<$Res> {
  _$CrawlEvent_AuditedCopyWithImpl(this._self, this._then);

  final CrawlEvent_Audited _self;
  final $Res Function(CrawlEvent_Audited) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? errors = null,Object? warnings = null,Object? notices = null,Object? findings = null,}) {
  return _then(CrawlEvent_Audited(
errors: null == errors ? _self.errors : errors // ignore: cast_nullable_to_non_nullable
as PlatformInt64,warnings: null == warnings ? _self.warnings : warnings // ignore: cast_nullable_to_non_nullable
as PlatformInt64,notices: null == notices ? _self.notices : notices // ignore: cast_nullable_to_non_nullable
as PlatformInt64,findings: null == findings ? _self.findings : findings // ignore: cast_nullable_to_non_nullable
as PlatformInt64,
  ));
}


}

/// @nodoc


class CrawlEvent_Finished extends CrawlEvent {
  const CrawlEvent_Finished({required this.summary}): super._();
  

 final  CrawlSummary summary;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_FinishedCopyWith<CrawlEvent_Finished> get copyWith => _$CrawlEvent_FinishedCopyWithImpl<CrawlEvent_Finished>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Finished&&(identical(other.summary, summary) || other.summary == summary));
}


@override
int get hashCode {
    return Object.hash(runtimeType,summary);
}

@override
String toString() {
    return 'CrawlEvent.finished(summary: $summary)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_FinishedCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_FinishedCopyWith(CrawlEvent_Finished value, $Res Function(CrawlEvent_Finished) _then) = _$CrawlEvent_FinishedCopyWithImpl;
@useResult
$Res call({
 CrawlSummary summary
});




}
/// @nodoc
class _$CrawlEvent_FinishedCopyWithImpl<$Res>
    implements $CrawlEvent_FinishedCopyWith<$Res> {
  _$CrawlEvent_FinishedCopyWithImpl(this._self, this._then);

  final CrawlEvent_Finished _self;
  final $Res Function(CrawlEvent_Finished) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? summary = null,}) {
  return _then(CrawlEvent_Finished(
summary: null == summary ? _self.summary : summary // ignore: cast_nullable_to_non_nullable
as CrawlSummary,
  ));
}


}

/// @nodoc


class CrawlEvent_Failed extends CrawlEvent {
  const CrawlEvent_Failed({required this.message}): super._();
  

 final  String message;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@JsonKey(includeFromJson: false, includeToJson: false)
@pragma('vm:prefer-inline')
$CrawlEvent_FailedCopyWith<CrawlEvent_Failed> get copyWith => _$CrawlEvent_FailedCopyWithImpl<CrawlEvent_Failed>(this, _$identity);



@override
bool operator ==(Object other) {
    return identical(this, other) || (other.runtimeType == runtimeType&&other is CrawlEvent_Failed&&(identical(other.message, message) || other.message == message));
}


@override
int get hashCode {
    return Object.hash(runtimeType,message);
}

@override
String toString() {
    return 'CrawlEvent.failed(message: $message)';
}


}

/// @nodoc
abstract mixin class $CrawlEvent_FailedCopyWith<$Res> implements $CrawlEventCopyWith<$Res> {
  factory $CrawlEvent_FailedCopyWith(CrawlEvent_Failed value, $Res Function(CrawlEvent_Failed) _then) = _$CrawlEvent_FailedCopyWithImpl;
@useResult
$Res call({
 String message
});




}
/// @nodoc
class _$CrawlEvent_FailedCopyWithImpl<$Res>
    implements $CrawlEvent_FailedCopyWith<$Res> {
  _$CrawlEvent_FailedCopyWithImpl(this._self, this._then);

  final CrawlEvent_Failed _self;
  final $Res Function(CrawlEvent_Failed) _then;

/// Create a copy of CrawlEvent
/// with the given fields replaced by the non-null parameter values.
@pragma('vm:prefer-inline') $Res call({Object? message = null,}) {
  return _then(CrawlEvent_Failed(
message: null == message ? _self.message : message // ignore: cast_nullable_to_non_nullable
as String,
  ));
}


}

// dart format on
